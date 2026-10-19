import { Database, NodbliteError, Errors, type PageResult } from '../src/index.js';

interface User { _id?: string; name: string; age: number; tags?: string[] }

const db = new Database('app.nodb');
const id: string = db.insert<User>('users', { name: '张三', age: 30 });

const adults: User[] = db.find<User>('users', { age: { $gte: 18 } }, { sort: { age: -1 }, limit: 10 });
const page: PageResult<User> = db.findPage<User>('users', { tags: { $all: ['a'] } });
const one: User | null = db.get<User>('users', id);

const n: number = db.count<User>('users', { age: { $exists: true } });
const names: string[] = db.collections();

db.createIndex('users', 'age_1', 'age', { unique: false });
db.createIndex('users', 'a_b', 'a', { fields: ['a', 'b'], directions: [1, -1] });
const plan = db.explain('users', { age: { $gte: 18 } });
const stage: 'IXSCAN' | 'COLLSCAN' = plan.stage;

db.update<User>('users', { name: '张三' }, { $inc: { age: 1 }, $push: { tags: 'x' } });
db.upsert<User>('users', { name: '李四' }, { $set: { age: 20 } });

try { db.insert('users', { name: 'x', age: 1 }); }
catch (e) { if (e instanceof Errors.DuplicateKeyError) void e.code; }

db.close();
void [id, adults, page.total, one, n, names, stage];
