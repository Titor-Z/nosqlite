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

// ---------- B 阶段:角色授权 ----------

// 用 root 创建指定角色的账号,返回带账号密码的连接串。
func createUserWithRole(t *testing.T, uri, user, pwd, role string) string {
	t.Helper()
	client, ctx, cancel, err := connectWith(uri)
	if err != nil {
		t.Fatalf("root 连接失败: %v", err)
	}
	defer cancel()
	defer client.Disconnect(context.Background())
	admin := client.Database("admin")
	_ = admin.RunCommand(ctx, bson.D{{Key: "dropUser", Value: user}}).Err()
	err = admin.RunCommand(ctx, bson.D{
		{Key: "createUser", Value: user},
		{Key: "pwd", Value: pwd},
		{Key: "roles", Value: bson.A{bson.M{"role": role, "db": "admin"}}},
	}).Err()
	if err != nil {
		t.Fatalf("createUser(%s, %s) 失败: %v", user, role, err)
	}
	return replaceUser(uri, user, pwd)
}

// 只读账号:能读,不能写,不能管索引。
func TestReadOnlyUser(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	viewer := createUserWithRole(t, uri, "viewer", "v_pwd", "read")

	client, ctx, cancel, err := connectWith(viewer)
	if err != nil {
		t.Fatalf("只读账号应能连接: %v", err)
	}
	defer cancel()
	defer client.Disconnect(context.Background())

	coll := client.Database("authdb").Collection("items")
	if _, err := coll.InsertOne(ctx, bson.M{"n": 1}); err == nil {
		t.Fatal("只读账号不应能插入")
	}
	if _, err := coll.UpdateOne(ctx, bson.M{}, bson.M{"$set": bson.M{"n": 2}}); err == nil {
		t.Fatal("只读账号不应能更新")
	}
	if _, err := coll.DeleteOne(ctx, bson.M{}); err == nil {
		t.Fatal("只读账号不应能删除")
	}
	if err := client.Database("authdb").RunCommand(ctx, bson.D{{Key: "createIndexes", Value: "items"}, {Key: "indexes", Value: bson.A{bson.M{"key": bson.M{"n": 1}, "name": "n_1"}}}}).Err(); err == nil {
		t.Fatal("只读账号不应能建索引")
	}
	// 读本身必须可用
	n, err := coll.CountDocuments(ctx, bson.M{})
	if err != nil || n < 0 {
		t.Fatalf("只读账号应能 count: %v", err)
	}
}

// readWrite 不能建账号;userAdmin 可以。
func TestUserAdminRole(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	rw := createUserWithRole(t, uri, "rw_user", "rw_pwd", "readWrite")
	ua := createUserWithRole(t, uri, "ua_user", "ua_pwd", "userAdmin")

	// readWrite → createUser 被拒
	c1, ctx1, cancel1, err := connectWith(rw)
	if err != nil {
		t.Fatalf("readWrite 账号应能连接: %v", err)
	}
	defer cancel1()
	defer c1.Disconnect(context.Background())
	if err := c1.Database("admin").RunCommand(ctx1, bson.D{
		{Key: "createUser", Value: "nope"},
		{Key: "pwd", Value: "x"},
		{Key: "roles", Value: bson.A{}},
	}).Err(); err == nil {
		t.Fatal("readWrite 账号不应能 createUser")
	}

	// userAdmin → createUser 成功,且 usersInfo 能看别人
	c2, ctx2, cancel2, err := connectWith(ua)
	if err != nil {
		t.Fatalf("userAdmin 账号应能连接: %v", err)
	}
	defer cancel2()
	defer c2.Disconnect(context.Background())
	if err := c2.Database("admin").RunCommand(ctx2, bson.D{
		{Key: "createUser", Value: "temp_ua"},
		{Key: "pwd", Value: "temp_pwd"},
		{Key: "roles", Value: bson.A{}},
	}).Err(); err != nil {
		t.Fatalf("userAdmin 应能 createUser: %v", err)
	}
	var info struct {
		Users []bson.M `bson:"users"`
	}
	if err := c2.Database("admin").RunCommand(ctx2, bson.D{{Key: "usersInfo", Value: "viewer"}}).Decode(&info); err != nil {
		t.Fatalf("userAdmin 应能看别人的 usersInfo: %v", err)
	}
	if len(info.Users) != 1 {
		t.Fatalf("usersInfo 应返回 1 个账号,实际 %d", len(info.Users))
	}
	if _, leaked := info.Users[0]["credentials"]; leaked {
		t.Fatal("usersInfo 不应泄漏 credentials")
	}
	// 清理
	_ = c2.Database("admin").RunCommand(ctx2, bson.D{{Key: "dropUser", Value: "temp_ua"}}).Err()
}

// $lookup 不得 join 出 admin.system.users 的凭据。
func TestLookupIntoSystemUsersDenied(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	root, ctx, cancel, err := connectWith(uri)
	if err != nil {
		t.Fatalf("root 连接失败: %v", err)
	}
	defer cancel()
	defer root.Disconnect(context.Background())

	src := root.Database("authdb").Collection("src")
	_ = src.Drop(context.Background())
	if _, err := src.InsertOne(ctx, bson.M{"x": "a"}); err != nil {
		t.Fatalf("root 建源集合失败: %v", err)
	}

	viewer := createUserWithRole(t, uri, "viewer2", "v2_pwd", "read")
	c, ctx2, cancel2, err := connectWith(viewer)
	if err != nil {
		t.Fatalf("只读账号应能连接: %v", err)
	}
	defer cancel2()
	defer c.Disconnect(context.Background())

	pipeline := bson.A{bson.M{
		"$lookup": bson.M{
			"from":         "admin.system.users",
			"localField":   "x",
			"foreignField": "_id",
			"as":           "u",
		},
	}}
	if err := c.Database("authdb").RunCommand(ctx2, bson.D{
		{Key: "aggregate", Value: "src"},
		{Key: "pipeline", Value: pipeline},
		{Key: "cursor", Value: bson.M{}},
	}).Err(); err == nil {
		t.Fatal("$lookup 指向 admin.system.users 必须被拒绝")
	}
}

// distinct / aggregate 不得在 admin.system.users 上读凭据。
func TestSystemUsersReadBypassDenied(t *testing.T) {
	uri := authURI()
	if uri == "" {
		t.Skip("缺少 NOSQLITE_AUTH_URI,跳过认证验收")
	}
	root, _, cancel, err := connectWith(uri)
	if err != nil {
		t.Fatalf("root 连接失败: %v", err)
	}
	defer cancel()
	defer root.Disconnect(context.Background())

	viewer := createUserWithRole(t, uri, "viewer3", "v3_pwd", "read")
	c, ctx2, cancel2, err := connectWith(viewer)
	if err != nil {
		t.Fatalf("只读账号应能连接: %v", err)
	}
	defer cancel2()
	defer c.Disconnect(context.Background())

	// distinct credentials
	if err := c.Database("admin").RunCommand(ctx2, bson.D{
		{Key: "distinct", Value: "system.users"},
		{Key: "key", Value: "credentials"},
	}).Err(); err == nil {
		t.Fatal("distinct 不应能读 system.users 的字段值")
	}
	// aggregate 直接打在 system.users 上
	if err := c.Database("admin").RunCommand(ctx2, bson.D{
		{Key: "aggregate", Value: "system.users"},
		{Key: "pipeline", Value: bson.A{}},
		{Key: "cursor", Value: bson.M{}},
	}).Err(); err == nil {
		t.Fatal("aggregate 不应能在 system.users 上执行")
	}
	// find 允许,但凭据必须被剥掉
	cur, err := c.Database("admin").Collection("system.users").Find(ctx2, bson.M{})
	if err != nil {
		t.Fatalf("find system.users 应放行(凭据已剥): %v", err)
	}
	var docs []bson.M
	if err = cur.All(ctx2, &docs); err != nil {
		t.Fatalf("解码失败: %v", err)
	}
	if len(docs) == 0 {
		t.Fatal("system.users 应有账号")
	}
	for _, d := range docs {
		if _, ok := d["credentials"]; ok {
			t.Fatal("find 返回的文档不应含 credentials")
		}
	}
}
