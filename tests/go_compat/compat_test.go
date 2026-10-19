// nosqlite-server 的 Go 驱动验收测试。
//
// 这是阶段 2 的**真正验收标准**:用 MongoDB 官方 Go 驱动连接 nosqlite-server,
// 走完整协议栈(OP_MSG + BSON + 游标),而不是自己造的客户端。
//
// 运行(由 tests/mongo_wire.rs 驱动,或手动):
//     go test -v ./go_compat/...
package gocompat

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"

	"go.mongodb.org/mongo-driver/v2/bson"
	"go.mongodb.org/mongo-driver/v2/mongo"
	"go.mongodb.org/mongo-driver/v2/mongo/options"
)

// 连接地址由环境变量注入(测试驱动起服务后传入)
func uri() string {
	if u := os.Getenv("NOSQLITE_URI"); u != "" {
		return u
	}
	return "mongodb://127.0.0.1:27017"
}

func connect(t *testing.T) (*mongo.Client, context.Context, context.CancelFunc) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	client, err := mongo.Connect(options.Client().ApplyURI(uri()))
	if err != nil {
		cancel()
		t.Fatalf("连接失败: %v", err)
	}
	// Ping 会触发完整握手(hello),是最早能发现协议问题的点
	if err := client.Ping(ctx, nil); err != nil {
		cancel()
		t.Fatalf("Ping 失败(握手不兼容): %v", err)
	}
	return client, ctx, cancel
}

// 握手 + Ping:驱动能连上即说明 hello 响应字段足够。
func TestConnect(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	var result bson.M
	if err := client.Database("admin").RunCommand(ctx, bson.D{bson.E{Key: "ping", Value: 1}}).Decode(&result); err != nil {
		t.Fatalf("ping 命令失败: %v", err)
	}
	t.Logf("ping -> %v", result)
}

// 插入 + 查询:验证 BSON 编解码与 ObjectId 映射。
func TestInsertAndFind(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("users")
	// 每轮用独立集合名,避免多次运行互相干扰
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	res, err := coll.InsertOne(ctx, bson.M{"name": "张三", "age": 30})
	if err != nil {
		t.Fatalf("InsertOne 失败: %v", err)
	}
	if res.InsertedID == nil {
		t.Fatal("InsertedID 为空")
	}
	t.Logf("inserted id = %v (%T)", res.InsertedID, res.InsertedID)

	// 用驱动返回的 ObjectId 查回来 —— 验证 _id 的 ObjectId <-> hex 字符串映射
	var got bson.M
	err = coll.FindOne(ctx, bson.M{"_id": res.InsertedID}).Decode(&got)
	if err != nil {
		t.Fatalf("FindOne 失败: %v", err)
	}
	if got["name"] != "张三" {
		t.Fatalf("字段不匹配: %v", got)
	}
	t.Logf("found = %v", got)
}

// 批量插入 + 游标遍历:验证 find 的 cursor 结构与分页。
func TestManyAndCursor(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("items")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 25)
	for i := 0; i < 25; i++ {
		docs = append(docs, bson.M{"n": i, "tag": fmt.Sprintf("t%d", i%3)})
	}
	res, err := coll.InsertMany(ctx, docs)
	if err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}
	if len(res.InsertedIDs) != 25 {
		t.Fatalf("应插入 25 条,实际 %d", len(res.InsertedIDs))
	}

	// 条件 + 排序 + 分页
	cur, err := coll.Find(ctx,
		bson.M{"n": bson.M{"$gte": 5}},
		options.Find().SetSort(bson.D{bson.E{Key: "n", Value: -1}}).SetLimit(5),
	)
	if err != nil {
		t.Fatalf("Find 失败: %v", err)
	}
	defer cur.Close(ctx)

	var results []bson.M
	if err := cur.All(ctx, &results); err != nil {
		t.Fatalf("游标遍历失败: %v", err)
	}
	if len(results) != 5 {
		t.Fatalf("应返回 5 条,实际 %d", len(results))
	}
	// 降序:首条应是 n=24
	if n, _ := results[0]["n"].(int32); n != 24 {
		t.Fatalf("排序不正确,首条 n=%v (类型 %T)", results[0]["n"], results[0]["n"])
	}
	for i := 1; i < len(results); i++ {
		prev, _ := results[i-1]["n"].(int32)
		curN, _ := results[i]["n"].(int32)
		if curN > prev {
			t.Fatalf("降序被破坏: %v 之后是 %v", prev, curN)
		}
	}
	t.Logf("cursor -> %d 条,首条 n=%v", len(results), results[0]["n"])
}

// 更新:验证 update 命令与 $inc/$set。
func TestUpdate(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("counters")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	_, err := coll.InsertOne(ctx, bson.M{"name": "c1", "views": 1})
	if err != nil {
		t.Fatalf("InsertOne 失败: %v", err)
	}

	// $inc + $set
	_, err = coll.UpdateOne(ctx,
		bson.M{"name": "c1"},
		bson.M{"$inc": bson.M{"views": 5}, "$set": bson.M{"vip": true}},
	)
	if err != nil {
		t.Fatalf("UpdateOne 失败: %v", err)
	}

	var got bson.M
	if err := coll.FindOne(ctx, bson.M{"name": "c1"}).Decode(&got); err != nil {
		t.Fatalf("FindOne 失败: %v", err)
	}
	// BSON 整数会被驱动解成 int32 或 int64
	var views int64
	switch v := got["views"].(type) {
	case int32:
		views = int64(v)
	case int64:
		views = v
	default:
		t.Fatalf("views 类型异常: %T", got["views"])
	}
	if views != 6 {
		t.Fatalf("views 应为 6,实际 %d", views)
	}
	if got["vip"] != true {
		t.Fatalf("vip 应为 true,实际 %v", got["vip"])
	}
}

// 删除:验证 delete 命令。
func TestDelete(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("todelete")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	_, err := coll.InsertMany(ctx, []interface{}{
		bson.M{"n": 1}, bson.M{"n": 2}, bson.M{"n": 3},
	})
	if err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	res, err := coll.DeleteMany(ctx, bson.M{"n": bson.M{"$lt": 3}})
	if err != nil {
		t.Fatalf("DeleteMany 失败: %v", err)
	}
	if res.DeletedCount != 2 {
		t.Fatalf("应删除 2 条,实际 %d", res.DeletedCount)
	}

	count, err := coll.CountDocuments(ctx, bson.M{})
	if err != nil {
		t.Fatalf("CountDocuments 失败: %v", err)
	}
	if count != 1 {
		t.Fatalf("剩余应为 1 条,实际 %d", count)
	}
}

// findAndModify:Mongoose findOneAndUpdate 的底层命令。
func TestFindOneAndUpdate(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("fam")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	_, err := coll.InsertOne(ctx, bson.M{"name": "x", "v": 1})
	if err != nil {
		t.Fatalf("InsertOne 失败: %v", err)
	}

	opts := options.FindOneAndUpdate().SetReturnDocument(options.After)
	var after bson.M
	err = coll.FindOneAndUpdate(ctx,
		bson.M{"name": "x"},
		bson.M{"$inc": bson.M{"v": 10}},
		opts,
	).Decode(&after)
	if err != nil {
		t.Fatalf("FindOneAndUpdate 失败: %v", err)
	}
	var v int64
	switch n := after["v"].(type) {
	case int32:
		v = int64(n)
	case int64:
		v = n
	}
	if v != 11 {
		t.Fatalf("v 应为 11,实际 %d", v)
	}
}

// upsert:未命中时插入。
func TestUpsert(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("upsert")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	opts := options.UpdateOne().SetUpsert(true)
	res, err := coll.UpdateOne(ctx,
		bson.M{"email": "a@b.c"},
		bson.M{"$set": bson.M{"name": "张三"}},
		opts,
	)
	if err != nil {
		t.Fatalf("upsert UpdateOne 失败: %v", err)
	}
	if res.UpsertedID == nil {
		t.Fatal("应返回 UpsertedID")
	}

	count, err := coll.CountDocuments(ctx, bson.M{})
	if err != nil {
		t.Fatalf("CountDocuments 失败: %v", err)
	}
	if count != 1 {
		t.Fatalf("应为 1 条,实际 %d", count)
	}
}

// Date 类型:驱动发 datetime,服务端以 {"$date": ms} 存储。
func TestDateType(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("events")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	now := time.Date(2026, 9, 16, 10, 0, 0, 0, time.UTC)
	_, err := coll.InsertOne(ctx, bson.M{"name": "发布", "at": now})
	if err != nil {
		t.Fatalf("插入 Date 失败: %v", err)
	}

	var got bson.M
	if err := coll.FindOne(ctx, bson.M{"name": "发布"}).Decode(&got); err != nil {
		t.Fatalf("FindOne 失败: %v", err)
	}
	at, ok := got["at"].(bson.DateTime)
	if !ok {
		t.Fatalf("at 应为 bson.DateTime,实际 %T", got["at"])
	}
	if !at.Time().Equal(now) {
		t.Fatalf("时间不匹配: 期望 %v,实际 %v", now, at.Time())
	}
}

// 错误路径:重复主键必须让驱动收到 DuplicateKey 错误码。
func TestDuplicateKey(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("dup")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	id := bson.NewObjectID()
	_, err := coll.InsertOne(ctx, bson.M{"_id": id, "n": 1})
	if err != nil {
		t.Fatalf("首次插入失败: %v", err)
	}
	_, err = coll.InsertOne(ctx, bson.M{"_id": id, "n": 2})
	if err == nil {
		t.Fatal("重复主键应报错")
	}
	if !mongo.IsDuplicateKeyError(err) {
		t.Fatalf("错误应为 DuplicateKey,实际: %v", err)
	}
}

// 真实游标:batchSize 分批 + 多次 getMore,驱动会自动发 getMore。
func TestCursorBatching(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("batched")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	// 120 条 > 首批 101 条,必然触发 getMore
	docs := make([]interface{}, 0, 120)
	for i := 0; i < 120; i++ {
		docs = append(docs, bson.M{"n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	// 小 batchSize 强制多次 getMore
	cur, err := coll.Find(ctx, bson.M{},
		options.Find().SetSort(bson.D{bson.E{Key: "n", Value: 1}}).SetBatchSize(7))
	if err != nil {
		t.Fatalf("Find 失败: %v", err)
	}
	defer cur.Close(ctx)

	count := 0
	lastN := int32(-1)
	for cur.Next(ctx) {
		var doc bson.M
		if err := cur.Decode(&doc); err != nil {
			t.Fatalf("Decode 失败: %v", err)
		}
		n, _ := doc["n"].(int32)
		if int32(lastN) >= 0 && n != lastN+1 {
			t.Fatalf("顺序不连续: %d 之后是 %d", lastN, n)
		}
		lastN = n
		count++
	}
	if err := cur.Err(); err != nil {
		t.Fatalf("游标遍历出错: %v", err)
	}
	if count != 120 {
		t.Fatalf("应遍历 120 条,实际 %d", count)
	}
}

// 游标提前关闭(killCursors)。
func TestCursorEarlyClose(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("closeme")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 200)
	for i := 0; i < 200; i++ {
		docs = append(docs, bson.M{"n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	cur, err := coll.Find(ctx, bson.M{}, options.Find().SetBatchSize(5))
	if err != nil {
		t.Fatalf("Find 失败: %v", err)
	}
	// 只读 3 条就关闭 → 触发 killCursors,不应报错
	for i := 0; i < 3 && cur.Next(ctx); i++ {
	}
	if err := cur.Close(ctx); err != nil {
		t.Fatalf("提前关闭游标失败: %v", err)
	}
}

// 聚合管道结果同样走游标。
func TestAggregateCursor(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("aggr")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 150)
	for i := 0; i < 150; i++ {
		docs = append(docs, bson.M{"n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	cur, err := coll.Aggregate(ctx, bson.A{
		bson.M{"$match": bson.M{"n": bson.M{"$gte": 100}}},
		bson.M{"$sort": bson.M{"n": 1}},
	}, options.Aggregate().SetBatchSize(10))
	if err != nil {
		t.Fatalf("Aggregate 失败: %v", err)
	}
	defer cur.Close(ctx)

	var results []bson.M
	if err := cur.All(ctx, &results); err != nil {
		t.Fatalf("聚合遍历失败: %v", err)
	}
	if len(results) != 50 {
		t.Fatalf("应返回 50 条,实际 %d", len(results))
	}
}

// limit 约束结果总数,不受 batchSize 影响。
func TestLimitWithBatchSize(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("limited")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 50)
	for i := 0; i < 50; i++ {
		docs = append(docs, bson.M{"n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	cur, err := coll.Find(ctx, bson.M{},
		options.Find().SetSort(bson.D{bson.E{Key: "n", Value: 1}}).
			SetLimit(12).SetBatchSize(5))
	if err != nil {
		t.Fatalf("Find 失败: %v", err)
	}
	defer cur.Close(ctx)

	var results []bson.M
	if err := cur.All(ctx, &results); err != nil {
		t.Fatalf("遍历失败: %v", err)
	}
	if len(results) != 12 {
		t.Fatalf("limit=12 应返回 12 条,实际 %d", len(results))
	}
}

// explain:验证 wire 层能返回查询计划(索引命中可见)。
func TestExplain(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("explainme")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 20)
	for i := 0; i < 20; i++ {
		docs = append(docs, bson.M{"n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	// 用 bson.M 解码,再逐层断言(嵌套类型统一)
	cmd := bson.D{
		bson.E{Key: "explain", Value: bson.M{
			"find":   "explainme",
			"filter": bson.M{"n": bson.M{"$gte": 10}},
		}},
	}
	var res bson.M
	if err := client.Database("gocompat").RunCommand(ctx, cmd).Decode(&res); err != nil {
		t.Fatalf("explain 失败: %v", err)
	}

	// 递归找 winningPlan(嵌套类型可能是 bson.M 或 bson.D)
	found := findStage(res)
	if found == "" {
		t.Fatalf("未找到 stage: %v", res)
	}
	if found != "COLLSCAN" {
		t.Fatalf("无索引应为 COLLSCAN,实际 %s", found)
	}
	t.Logf("explain stage = %s", found)

	// 建索引后 explain 应变成 IXSCAN
	name, err := coll.Indexes().CreateOne(ctx, mongo.IndexModel{
		Keys: bson.D{bson.E{Key: "n", Value: 1}},
	})
	if err != nil {
		t.Fatalf("createIndexes 失败: %v", err)
	}
	t.Logf("已创建索引: %s", name)

	var after bson.M
	if err := client.Database("gocompat").RunCommand(ctx, cmd).Decode(&after); err != nil {
		t.Fatalf("建索引后 explain 失败: %v", err)
	}
	if stage := findStage(after); stage != "IXSCAN" {
		t.Fatalf("建索引后应为 IXSCAN,实际 %s", stage)
	}

	// listIndexes 应能看到 _id_ 与 n_1
	cur, err := coll.Indexes().List(ctx)
	if err != nil {
		t.Fatalf("listIndexes 失败: %v", err)
	}
	var specs []bson.M
	if err := cur.All(ctx, &specs); err != nil {
		t.Fatalf("读取索引清单失败: %v", err)
	}
	if len(specs) < 2 {
		t.Fatalf("应至少有 _id_ 与 n_1 两个索引,实际 %d", len(specs))
	}

	// dropIndexes 后回到 COLLSCAN
	if err := coll.Indexes().DropOne(ctx, name); err != nil {
		t.Fatalf("dropIndexes 失败: %v", err)
	}
	var dropped bson.M
	if err := client.Database("gocompat").RunCommand(ctx, cmd).Decode(&dropped); err != nil {
		t.Fatalf("删索引后 explain 失败: %v", err)
	}
	if stage := findStage(dropped); stage != "COLLSCAN" {
		t.Fatalf("删索引后应回到 COLLSCAN,实际 %s", stage)
	}
}

// 复合索引经 wire 层创建后可用于前缀查询。
func TestCompositeIndexOverWire(t *testing.T) {
	client, ctx, cancel := connect(t)
	defer cancel()
	defer client.Disconnect(ctx)

	coll := client.Database("gocompat").Collection("wirecomposite")
	_ = coll.Drop(ctx)
	t.Cleanup(func() { _ = coll.Drop(context.Background()) })

	docs := make([]interface{}, 0, 50)
	for i := 0; i < 50; i++ {
		docs = append(docs, bson.M{"tenant": i % 5, "n": i})
	}
	if _, err := coll.InsertMany(ctx, docs); err != nil {
		t.Fatalf("InsertMany 失败: %v", err)
	}

	_, err := coll.Indexes().CreateOne(ctx, mongo.IndexModel{
		Keys: bson.D{bson.E{Key: "tenant", Value: 1}, bson.E{Key: "n", Value: 1}},
	})
	if err != nil {
		t.Fatalf("创建复合索引失败: %v", err)
	}

	cmd := bson.D{
		bson.E{Key: "explain", Value: bson.M{
			"find":   "wirecomposite",
			"filter": bson.M{"tenant": 1},
		}},
	}
	var res bson.M
	if err := client.Database("gocompat").RunCommand(ctx, cmd).Decode(&res); err != nil {
		t.Fatalf("explain 失败: %v", err)
	}
	if stage := findStage(res); stage != "IXSCAN" {
		t.Fatalf("复合索引前缀查询应为 IXSCAN,实际 %s", stage)
	}

	n, err := coll.CountDocuments(ctx, bson.M{"tenant": 1})
	if err != nil {
		t.Fatalf("CountDocuments 失败: %v", err)
	}
	if n != 10 {
		t.Fatalf("tenant=1 应有 10 条,实际 %d", n)
	}
}

// findStage 在任意嵌套结构中查找 `stage` 字段。
func findStage(v interface{}) string {
	switch doc := v.(type) {
	case bson.M:
		if s, ok := doc["stage"].(string); ok {
			return s
		}
		for _, child := range doc {
			if s := findStage(child); s != "" {
				return s
			}
		}
	case bson.D:
		for _, e := range doc {
			if e.Key == "stage" {
				if s, ok := e.Value.(string); ok {
					return s
				}
			}
			if s := findStage(e.Value); s != "" {
				return s
			}
		}
	case []interface{}:
		for _, child := range doc {
			if s := findStage(child); s != "" {
				return s
			}
		}
	}
	return ""
}
