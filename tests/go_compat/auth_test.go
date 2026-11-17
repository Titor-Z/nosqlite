// nodblite-server 账号认证验收(SCRAM-SHA-256)。
//
// 与 compat_test.go 同样只依赖**官方 Go 驱动**:不自己造客户端,
// 否则「我们理解的 SCRAM」和「驱动理解的 SCRAM」错在同一处也测不出来。
//
// 需要环境变量 NOSQLITE_AUTH_URI(带账号密码的连接串);未提供时跳过,
// 这样常规的 go test ./... 不会因为缺账号而失败。
package gocompat

import (
	"context"
	"os"
	"testing"
	"time"

	"go.mongodb.org/mongo-driver/v2/bson"
	"go.mongodb.org/mongo-driver/v2/mongo"
	"go.mongodb.org/mongo-driver/v2/mongo/options"
)

// 带账号密码的连接串;为空表示本次不跑认证用例。
func authURI() string { return os.Getenv("NOSQLITE_AUTH_URI") }

// 用给定连接串连上去并 Ping(握手 + 认证都会在这里发生)。
func connectWith(uri string) (*mongo.Client, context.Context, context.CancelFunc, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	client, err := mongo.Connect(options.Client().ApplyURI(uri))
	if err == nil {
		err = client.Ping(ctx, nil)
	}
	return client, ctx, cancel, err
}

// 正确账号密码应能连上并正常读写。
func TestAuthCRUD(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}

	client, ctx, cancel, err := connectWith(uri)
	if err != nil {
		t.Fatalf("带账号连接失败: %v", err)
	}
	defer cancel()
	defer client.Disconnect(context.Background())

	coll := client.Database("authdb").Collection("items")
	defer coll.Drop(context.Background())

	if _, err := coll.InsertOne(ctx, bson.M{"n": 1}); err != nil {
		t.Fatalf("认证后插入失败: %v", err)
	}
	n, err := coll.CountDocuments(ctx, bson.M{})
	if err != nil {
		t.Fatalf("认证后 count 失败: %v", err)
	}
	if n != 1 {
		t.Fatalf("应有 1 条文档,实际 %d", n)
	}
}

// 密码错误必须被拒绝。
func TestAuthWrongPassword(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	bad := replacePassword(uri, "definitely-wrong-password")
	client, _, cancel, err := connectWith(bad)
	defer cancel()
	if err == nil {
		client.Disconnect(context.Background())
		t.Fatal("错误密码不应连上")
	}
}

// 库内已有用户时,未认证连接可以握手/健康探活,但**不能读写数据**。
//
// 与 MongoDB 一致:`hello` / `ping` 属于免认证命令(驱动与监控都依赖),
// 真正的闸门在数据命令上。
func TestAuthRequired(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	client, ctx, cancel, err := connectWith(stripCredentials(uri))
	defer cancel()
	if err != nil {
		// 有些驱动版本在握手阶段就判定失败,同样算通过
		return
	}
	defer client.Disconnect(context.Background())

	coll := client.Database("authdb").Collection("items")
	if _, err := coll.Find(ctx, bson.M{}); err == nil {
		t.Fatal("未认证连接不应能读数据")
	}
	if _, err := coll.InsertOne(ctx, bson.M{"n": 99}); err == nil {
		t.Fatal("未认证连接不应能写数据")
	}
}

// 服务端账号管理:createUser / usersInfo / dropUser 走 wire 协议。
func TestAuthCreateAndDropUser(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	client, ctx, cancel, err := connectWith(uri)
	if err != nil {
		t.Fatalf("连接失败: %v", err)
	}
	defer cancel()
	defer client.Disconnect(context.Background())

	admin := client.Database("admin")
	// 先删掉可能残留的账号,保证可重复运行
	_ = admin.RunCommand(ctx, bson.D{{Key: "dropUser", Value: "temp_user"}}).Err()

	if err := admin.RunCommand(ctx, bson.D{
		{Key: "createUser", Value: "temp_user"},
		{Key: "pwd", Value: "temp_pwd"},
		{Key: "roles", Value: bson.A{}},
	}).Err(); err != nil {
		t.Fatalf("createUser 失败: %v", err)
	}

	// usersInfo 必须能看到(且不含密码摘要)
	var info struct {
		Users []bson.M `bson:"users"`
	}
	if err := admin.RunCommand(ctx, bson.D{{Key: "usersInfo", Value: "temp_user"}}).Decode(&info); err != nil {
		t.Fatalf("usersInfo 失败: %v", err)
	}
	if len(info.Users) != 1 {
		t.Fatalf("usersInfo 应返回 1 个账号,实际 %d", len(info.Users))
	}
	if _, leaked := info.Users[0]["credentials"]; leaked {
		t.Fatal("usersInfo 不应泄漏 credentials")
	}

	// 新账号能独立连上
	newURI := replaceUser(uri, "temp_user", "temp_pwd")
	c2, _, cancel2, err := connectWith(newURI)
	if err != nil {
		t.Fatalf("新账号连接失败: %v", err)
	}
	cancel2()
	c2.Disconnect(context.Background())

	if err := admin.RunCommand(ctx, bson.D{{Key: "dropUser", Value: "temp_user"}}).Err(); err != nil {
		t.Fatalf("dropUser 失败: %v", err)
	}
}

// ---------- 连接串改写(只用于构造反例) ----------

// replacePassword 把 `mongodb://u:p@h` 里的 p 换掉。
func replacePassword(uri, pwd string) string {
	user, host := splitCreds(uri)
	return "mongodb://" + user + ":" + pwd + "@" + host
}

// replaceUser 换用户名与密码。
func replaceUser(uri, user, pwd string) string {
	_, host := splitCreds(uri)
	return "mongodb://" + user + ":" + pwd + "@" + host
}

// stripCredentials 去掉账号部分。
func stripCredentials(uri string) string {
	_, host := splitCreds(uri)
	return "mongodb://" + host
}

// splitCreds 把 `mongodb://u:p@host/...` 拆成 (u, host/...) 。
func splitCreds(uri string) (string, string) {
	rest := uri
	for _, prefix := range []string{"mongodb://", "mongodb+srv://"} {
		if len(rest) > len(prefix) && rest[:len(prefix)] == prefix {
			rest = rest[len(prefix):]
			break
		}
	}
	at := -1
	for i := 0; i < len(rest); i++ {
		if rest[i] == '@' {
			at = i
			break
		}
	}
	if at < 0 {
		return "", rest
	}
	creds := rest[:at]
	host := rest[at+1:]
	colon := -1
	for i := 0; i < len(creds); i++ {
		if creds[i] == ':' {
			colon = i
			break
		}
	}
	if colon < 0 {
		return creds, host
	}
	return creds[:colon], host
}
