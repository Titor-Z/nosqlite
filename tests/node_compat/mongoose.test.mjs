// nodblite wire 协议的 Mongoose 验收。
//
// 与 compat.test.mjs 对齐:不自己造客户端,用社区最常用的 ODM 走完整协议栈。
// 重点验证「ORM 层面的隐性调用」:建模时的 createIndexes、timestamps 的 Date 往返、
// ref + populate 的 ObjectId 翻译、countDocuments 背后的 aggregate/$group。

import { strict as assert } from 'node:assert';
import test from 'node:test';
import mongoose from 'mongoose';

const uri = process.env.NOSQLITE_URI;
if (!uri) {
  console.error('缺少环境变量 NOSQLITE_URI');
  process.exit(1);
}

// ---------------------------- 模型定义 ----------------------------

/** 作者:被 Post.ref 引用,验证 populate 的双向翻译。 */
const Author = mongoose.model(
  'Author',
  new mongoose.Schema(
    {
      name: { type: String, required: true },
      email: { type: String, index: true },
    },
    { timestamps: true },
  ),
);

/** 文章:含 ref、数值运算符、复合索引。 */
const Post = mongoose.model(
  'Post',
  new mongoose.Schema(
    {
      title: { type: String, required: true },
      views: { type: Number, default: 0 },
      tags: [String],
      author: { type: mongoose.Schema.Types.ObjectId, ref: 'Author' },
      publishedAt: Date,
    },
    { timestamps: true },
  ),
);
Post.schema.index({ author: 1, publishedAt: -1 });

test.before(async () => {
  await mongoose.connect(uri, { dbName: 'mongoosecompat' });
});

test.after(async () => {
  await mongoose.disconnect();
});

/** 每个用例前清空集合。 */
async function fresh() {
  await Promise.all([Author.deleteMany({}), Post.deleteMany({})]);
}

// ---------------------------- 建模与索引 ----------------------------

test('建模:tokens 生成与索引建立', async () => {
  await fresh();
  // Model.init() 会真正发 createIndexes
  await Promise.all([Author.init(), Post.init()]);

  const indexes = await Post.collection.indexes();
  const fields = indexes.map((i) => Object.keys(i.key).join(','));
  assert.ok(fields.includes('author,publishedAt'), `应有复合索引,实际 ${fields}`);
});

test('必填字段校验由 ODM 完成', async () => {
  await fresh();
  await assert.rejects(() => Post.create({ views: 1 }), /title/);
});

// ---------------------------- CRUD ----------------------------

test('create/save 与默认值、timestamps', async () => {
  await fresh();
  const post = await Post.create({ title: '第一篇', tags: ['rust'] });

  assert.equal(post.views, 0, '默认值应在 ODM 侧补齐');
  assert.ok(post._id, '应生成 ObjectId');
  assert.ok(post.createdAt instanceof Date, 'timestamps 应回读为 Date');

  // 重新查询确认 Date 真的落盘并被读回
  const found = await Post.findById(post._id);
  assert.ok(found.createdAt instanceof Date);
  assert.equal(found.title, '第一篇');
});

test('find 条件、排序与分页', async () => {
  await fresh();
  await Post.create([
    { title: 'a', views: 10, tags: ['x'] },
    { title: 'b', views: 30, tags: ['y'] },
    { title: 'c', views: 20, tags: ['x', 'y'] },
  ]);

  const top = await Post.find({ views: { $gte: 10 } }).sort({ views: -1 }).limit(2);
  assert.deepEqual(top.map((p) => p.title), ['b', 'c']);

  const byTag = await Post.find({ tags: 'x' }).sort({ views: 1 });
  assert.deepEqual(byTag.map((p) => p.title), ['a', 'c'], '数组包含语义');
});

test('$inc/$set 更新与 findOneAndUpdate', async () => {
  await fresh();
  const post = await Post.create({ title: 'p', tags: [] });

  await Post.updateOne({ _id: post._id }, { $inc: { views: 5 }, $set: { title: 'q' } });
  const after = await Post.findById(post._id);
  assert.equal(after.views, 5);
  assert.equal(after.title, 'q');

  const updated = await Post.findOneAndUpdate(
    { _id: post._id },
    { $inc: { views: 2 } },
    { new: true },
  );
  assert.equal(updated.views, 7, 'findOneAndUpdate 应返回新值');
});

test('countDocuments(底层是 aggregate/$group)', async () => {
  await fresh();
  await Post.create([{ title: '1' }, { title: '2' }, { title: '3' }]);
  assert.equal(await Post.countDocuments({}), 3);
  assert.equal(await Post.countDocuments({ title: '2' }), 1);
});

// ---------------------------- 关联 ----------------------------

test('ref + populate 的 ObjectId 翻译', async () => {
  await fresh();
  const author = await Author.create({ name: '张三', email: 'z@example.com' });
  await Post.create({ title: 'p1', author: author._id });
  await Post.create({ title: 'p2', author: author._id });

  const posts = await Post.find().populate('author').sort({ title: 1 });
  assert.equal(posts.length, 2);
  assert.equal(posts[0].author.name, '张三');
  assert.equal(posts[0].author._id.toString(), author._id.toString());
});

// ---------------------------- 原生通道 ----------------------------

test('aggregate 直通($match + $group)', async () => {
  await fresh();
  await Post.create([
    { title: 'a', views: 1, tags: ['x'] },
    { title: 'b', views: 2, tags: ['x'] },
    { title: 'c', views: 3, tags: ['y'] },
  ]);

  const rows = await Post.aggregate([
    { $match: { tags: 'x' } },
    { $group: { _id: '$title', total: { $sum: '$views' } } },
    { $sort: { _id: 1 } },
  ]);
  assert.deepEqual(
    rows.map((r) => [r._id, r.total]),
    [['a', 1], ['b', 2]],
  );
});

test('deleteMany 与集合清空', async () => {
  await fresh();
  await Post.create([{ title: '1' }, { title: '2' }]);
  const res = await Post.deleteMany({});
  assert.equal(res.deletedCount, 2);
  assert.equal(await Post.countDocuments({}), 0);
});
