/**
 * nodblite —— 单文件 NoSQL 文档数据库的 TypeScript 类型声明。
 *
 * 文档:https://titor-z.github.io/nosqlite/
 */

// ==================== 查询操作符 ====================

/** 比较操作符。 */
export interface ComparisonOps<T> {
  $eq?: T | ElementOf<T>;
  $ne?: T | ElementOf<T>;
  $gt?: T;
  $gte?: T;
  $lt?: T;
  $lte?: T;
  /** 命中任一;数组字段会展开元素比较。 */
  $in?: readonly (T | ElementOf<T>)[];
  $nin?: readonly (T | ElementOf<T>)[];
  $regex?: string | RegExp;
}

/** 元素与类型操作符。 */
export interface ElementOps {
  /** 字段是否存在。注意:与 `$eq: null` **不等价**。 */
  $exists?: boolean;
  /** BSON 类型名,可传数组。`"number"` 接受 int/long/double。 */
  $type?:
    | 'double'
    | 'string'
    | 'object'
    | 'array'
    | 'objectId'
    | 'bool'
    | 'date'
    | 'null'
    | 'int'
    | 'long'
    | 'number'
    | 'minKey'
    | 'maxKey'
    | 'timestamp'
    | 'regex'
    | Array<
        | 'double'
        | 'string'
        | 'object'
        | 'array'
        | 'objectId'
        | 'bool'
        | 'date'
        | 'null'
        | 'int'
        | 'long'
        | 'number'
        | 'minKey'
        | 'maxKey'
        | 'timestamp'
        | 'regex'
      >;
}

/** 取数组的元素类型;非数组则为自身。 */
type ElementOf<T> = T extends readonly (infer E)[] ? E : T;

/** 数组操作符。`$all` / `$elemMatch` 作用于**元素**,不是整个数组。 */
export interface ArrayOps<T> {
  /** 数组须**同时包含**全部元素。空数组不匹配任何文档。 */
  $all?: readonly ElementOf<T>[];
  /** 数组长度精确匹配。非数组不匹配。 */
  $size?: number;
  /** 同一元素须同时满足内部条件。 */
  $elemMatch?: Record<string, unknown> | ComparisonOps<unknown> | ElementOps;
}

/** 字段级操作符集合。 */
export type FieldQuery<T = unknown> =
  | T
  | ComparisonOps<T>
  | ElementOps
  | ArrayOps<T>
  | { $not: FieldQuery<T> };

/** 过滤条件。 */
export type Filter<T = Record<string, unknown>> = {
  [K in keyof T]?: FieldQuery<T[K]>;
} & {
  $and?: Filter<T>[];
  $or?: Filter<T>[];
  $nor?: Filter<T>[];
  $not?: Filter<T>;
  /** 点号路径,如 `"items.name"`(会穿透数组)。 */
  [path: string]: unknown;
};

/** 排序规格:`{ age: -1, name: 1 }`。 */
export type Sort = Record<string, 1 | -1>;

/**
 * 投影。
 * - 包含模式:`{ name: 1 }`
 * - 排除模式:`{ secret: 0 }`
 * - `$slice` / `$elemMatch` / 位置运算符 `"arr.$"`
 */
export type Projection = Record<
  string,
  | 0
  | 1
  | boolean
  | { $slice: number | [number, number] }
  | { $elemMatch: Record<string, unknown> }
>;

// ==================== 更新操作符 ====================

export interface UpdateOperators<T = Record<string, unknown>> {
  /** 赋值,支持点号路径,嵌套自动创建。 */
  $set?: Partial<Record<keyof T | string, unknown>>;
  /** 删除字段;数组元素置 null。 */
  $unset?: Partial<Record<keyof T | string, unknown>>;
  /** 数值增减。 */
  $inc?: Partial<Record<keyof T | string, number>>;
  /** 数值乘法。 */
  $mul?: Partial<Record<keyof T | string, number>>;
  /** 取更小值。 */
  $min?: Partial<Record<keyof T | string, unknown>>;
  /** 取更大值。 */
  $max?: Partial<Record<keyof T | string, unknown>>;
  /** 字段改名。 */
  $rename?: Partial<Record<keyof T | string, string>>;
  /** 追加元素,支持 `$each`/`$position`/`$slice`/`$sort`。 */
  $push?: Partial<Record<keyof T | string, unknown>>;
  /** 追加(已存在则跳过)。 */
  $addToSet?: Partial<Record<keyof T | string, unknown>>;
  /** 移除**所有**匹配元素。 */
  $pull?: Partial<Record<keyof T | string, unknown>>;
  /** `1` 移除末尾,`-1` 移除开头。 */
  $pop?: Partial<Record<keyof T | string, 1 | -1>>;
  /** 写入当前时间。 */
  $currentDate?: Partial<Record<keyof T | string, true | { $type: 'date' | 'timestamp' }>>;
}

/** 更新文档。 */
export type Update<T = Record<string, unknown>> = UpdateOperators<T>;

// ==================== 查询选项 ====================

export interface FindOptions {
  sort?: Sort;
  fields?: Projection;
  skip?: number;
  limit?: number;
}

export interface PageResult<T> {
  /** 匹配总数(不受 skip/limit 影响)。 */
  total: number;
  /** 当页文档。 */
  items: T[];
}

// ==================== 索引 ====================

export interface IndexInfo {
  name: string;
  fields: string[];
  /** 与 fields 一一对应:`1` 升序,`-1` 降序。 */
  directions: number[];
  unique: boolean;
  sparse: boolean;
}

export interface CreateIndexOptions {
  unique?: boolean;
  sparse?: boolean;
  /** 传了则为复合索引,顺序即前缀顺序。 */
  fields?: string[];
  /** 与 fields 对应,默认全升序。 */
  directions?: number[];
}

/** 查询计划。 */
export interface ExplainResult {
  stage: 'IXSCAN' | 'COLLSCAN';
  collection?: string;
  indexName?: string;
  field?: string;
  bounds?: string;
  sortedByIndex?: boolean;
  filterApplied?: boolean;
  [key: string]: unknown;
}

// ==================== 错误 ====================

/** 所有 nodblite 错误的基类。 */
export class NodbliteError extends Error {
  /** 稳定错误码。 */
  code: number;
  /** 出错的操作名。 */
  op: string;
}

export class DuplicateKeyError extends NodbliteError {}
export class InvalidPathError extends NodbliteError {}
export class InvalidFormatError extends NodbliteError {}
export class VersionMismatchError extends NodbliteError {}
export class InvalidCollectionError extends NodbliteError {}
export class InvalidFilterError extends NodbliteError {}
export class InvalidUpdateError extends NodbliteError {}
export class InvalidSortError extends NodbliteError {}
export class InvalidProjectionError extends NodbliteError {}
export class InvalidDocumentError extends NodbliteError {}
export class IoError extends NodbliteError {}
export class InternalError extends NodbliteError {}

export declare const Errors: {
  DuplicateKeyError: typeof DuplicateKeyError;
  InvalidPathError: typeof InvalidPathError;
  InvalidFormatError: typeof InvalidFormatError;
  VersionMismatchError: typeof VersionMismatchError;
  InvalidCollectionError: typeof InvalidCollectionError;
  InvalidFilterError: typeof InvalidFilterError;
  InvalidUpdateError: typeof InvalidUpdateError;
  InvalidSortError: typeof InvalidSortError;
  InvalidProjectionError: typeof InvalidProjectionError;
  InvalidDocumentError: typeof InvalidDocumentError;
  IoError: typeof IoError;
  InternalError: typeof InternalError;
};

// ==================== 主类 ====================

export interface DatabaseOptions {
  /** 文件不存在时是否创建,默认 `true`。 */
  create?: boolean;
  /** 显式指定动态库路径。 */
  libPath?: string;
}

export class Database {
  /** 数据库文件路径。 */
  readonly path: string;

  /**
   * 打开一个 `.nodb` 数据库。
   * @throws {InvalidPathError} 后缀不是 `.nodb`
   */
  constructor(path: string, opts?: DatabaseOptions);

  // ---------- 读 ----------

  /** 查询文档。 */
  find<T = Record<string, unknown>>(
    collection: string,
    filter?: Filter<T>,
    opts?: FindOptions,
  ): T[];

  /** 分页:一次返回「匹配总数 + 当页文档」。 */
  findPage<T = Record<string, unknown>>(
    collection: string,
    filter?: Filter<T>,
    opts?: FindOptions,
  ): PageResult<T>;

  /** 按 `_id` 读单条,不存在返回 `null`。 */
  get<T = Record<string, unknown>>(collection: string, id: string): T | null;

  /** 计数。 */
  count<T = Record<string, unknown>>(collection: string, filter?: Filter<T>): number;

  /** 列出全部集合名。 */
  collections(): string[];

  // ---------- 写 ----------

  /** 插入一条,返回 `_id`。 */
  insert<T extends object>(collection: string, doc: T): string;

  /** 批量插入(单事务),返回 `_id` 数组。 */
  insertMany<T extends object>(collection: string, docs: T[]): string[];

  /** 更新,返回实际修改条数。 */
  update<T = Record<string, unknown>>(
    collection: string,
    filter: Filter<T>,
    update: Update<T>,
  ): number;

  /** 删除,返回删除条数。 */
  delete<T = Record<string, unknown>>(collection: string, filter: Filter<T>): number;

  /** 更新或插入。 */
  upsert<T = Record<string, unknown>>(
    collection: string,
    filter: Filter<T>,
    update: Update<T>,
  ): { updated: number } | { inserted: string };

  // ---------- 索引 ----------

  /** 创建索引(单字段或复合)。 */
  createIndex(
    collection: string,
    name: string,
    field: string,
    opts?: CreateIndexOptions,
  ): string;

  /** 列出索引定义。 */
  listIndexes(collection: string): IndexInfo[];

  /** 删除索引,返回是否确实删除了。 */
  dropIndex(collection: string, name: string): boolean;

  /** 查看查询计划。 */
  explain(
    collection: string,
    filter?: Record<string, unknown>,
    sort?: Sort | null,
  ): ExplainResult;

  // ---------- 生命周期 ----------

  /** 关闭数据库。可重复调用。 */
  close(): void;

  /** 支持 `using db = new Database(...)` 自动关闭(TS 5.2+)。 */
  [Symbol.dispose](): void;
}

/** 解析动态库路径(调试用)。 */
export function resolveLibPath(): string;
