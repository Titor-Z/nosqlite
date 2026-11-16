/**
 * nodblite-studio 的浏览器冒烟测试(对标 Compass Data Explorer 的功能集)。
 *
 * 前置:先启动服务端
 *   cargo build --release --features studio --bin nodblite-studio
 *   ./target/release/nodblite-studio --addr 127.0.0.1:8799
 *
 * 运行:
 *   node studio-ui/e2e/smoke.mjs
 *
 * 依赖系统 Chrome(playwright 的新版 chromium 不支持 macOS 12)。
 */

import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const url = process.env.STUDIO_URL ?? 'http://127.0.0.1:8799';
const shot = process.env.SCREENSHOT ?? '';

let failures = 0;
function check(ok, label) {
  console.log(`${ok ? '✓' : '✗'} ${label}`);
  if (!ok) failures += 1;
}

const browser = await chromium.launch({ channel: 'chrome' });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.on('pageerror', (e) => console.error('[pageerror]', e.message));
page.on('console', (m) => {
  if (m.type() === 'error' && !m.text().includes('favicon')) console.error('[console]', m.text());
});

/** 生成 24 条测试文档。 */
const bulkDocs = Array.from({ length: 24 }, (_, i) => ({
  _id: `u${i + 2}`,
  name: `doc${String(i + 2).padStart(2, '0')}`,
  age: 20 + i,
  tags: ['x', 'y'],
}));

try {
  await page.goto(url, { waitUntil: 'networkidle' });

  // 1. WASM 引擎
  await page.waitForSelector('text=ENGINE READY', { timeout: 20000 });
  check(true, 'WASM 引擎加载完成');

  // 2. 新建数据库 → 工作台
  await page.getByRole('button', { name: '新建空数据库' }).click();
  await page.waitForSelector('text=DATABASES');
  check(true, '新建数据库并进入工作台');

  // 3. Insert Document
  await page.getByRole('button', { name: '+ Insert Document' }).click();
  await page.getByPlaceholder('users').fill('users');
  await page.locator('div.panel textarea').fill(
    '{"_id":"u1","name":"alpha","age":19,"tags":["a","b"]}',
  );
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Fields' }).click();
  check((await page.locator('input[placeholder=字段名]').count()) === 4, 'Fields 视图按 JSON 生成字段行');
  await dialog.getByRole('button', { name: 'JSON', exact: true }).click();
  await dialog.getByRole('button', { name: '保存' }).click();
  await page.waitForSelector('text=alpha');
  check(true, 'Insert Document(自动创建集合)');

  // 4. Insert Documents(批量)
  await page.getByRole('button', { name: 'Add Data ▾' }).click();
  await page.getByRole('button', { name: 'Insert Documents', exact: true }).click();
  await page.locator('div.panel textarea').fill(JSON.stringify(bulkDocs));
  await page.getByRole('button', { name: '保存' }).click();
  await page.getByText('已插入 24 条文档').waitFor();
  check(true, 'Insert Documents 批量插入');

  // 5. Table 视图:列并集 + 嵌套展开
  const headers = await page.locator('table thead th').allInnerTexts();
  check(
    ['_id', 'name', 'age', 'tags'].every((h) => headers.some((x) => x.includes(h))),
    'Table 视图按字段并集生成列',
  );
  await page.getByRole('button', { name: '[2]' }).first().click();
  await page.locator('pre.json-block').first().waitFor();
  check(true, '数组单元格行内展开');

  // 6. 列头排序
  await page.getByRole('button', { name: /^age/ }).first().click();
  await page.waitForTimeout(300);
  check(
    (await page.getByLabel('Sort').inputValue()).includes('"age"'),
    '列头点击写入 Sort',
  );

  // 7. 范围分页
  await page.getByText('Documents').first().waitFor();
  check((await page.locator('table tbody tr').count()) === 20, '首批显示 20 条');
  await page.getByRole('button', { name: '›' }).click();
  await page.waitForTimeout(400);
  check((await page.locator('table tbody tr').count()) === 5, '下一页显示剩余 5 条');

  // 8. JSON 视图
  await page.getByRole('button', { name: 'JSON', exact: true }).click();
  check((await page.locator('main article').count()) === 5, 'JSON 视图渲染当前页');
  await page.getByRole('button', { name: 'Table', exact: true }).click();
  await page.getByRole('button', { name: '‹' }).click();
  await page.waitForTimeout(300);

  // 9. 复制文档
  const firstRow = page.locator('table tbody tr').first();
  await firstRow.hover();
  await firstRow.getByRole('button', { name: '复制' }).click();
  await page.getByText('文档已复制').waitFor();
  check(true, '复制文档(新 _id)');

  // 10. 建索引
  await page.getByRole('button', { name: 'Indexes', exact: true }).click();
  await page.getByRole('button', { name: 'Create Index' }).click();
  await page.getByPlaceholder('字段路径,如 tenant 或 a.b').fill('age');
  await page.getByRole('button', { name: '创建索引' }).click();
  await page.locator('text=age_1').first().waitFor();
  check(true, 'Index Builder 创建索引');

  // 11. Explain(IXSCAN)
  await page.getByRole('button', { name: 'Documents', exact: true }).click();
  await page.getByLabel('Filter').fill('{"age":{"$gte":20}}');
  await page.getByRole('button', { name: 'Find' }).click();
  await page.waitForTimeout(300);
  await page.getByRole('navigation').getByRole('button', { name: 'Explain' }).click();
  await page.getByText('WINNING PLAN').waitFor();
  check(
    (await page.locator('main').innerText()).includes('IXSCAN'),
    'Explain 显示 IXSCAN 计划',
  );
  check(
    (await page.locator('main').innerText()).includes('CLIENT MEASURED'),
    'Explain 标注 CLIENT MEASURED 指标',
  );

  // 12. Schema 采样
  await page.getByRole('button', { name: 'Schema', exact: true }).click();
  await page.getByRole('button', { name: 'Analyze' }).click();
  await page.getByText('个字段').first().waitFor();
  check((await page.locator('main').innerText()).includes('tags'), 'Schema 采样出字段');

  // 13. Aggregation
  await page.getByRole('button', { name: 'Aggregation', exact: true }).click();
  await page.locator('textarea').first().fill('{"$group":{"_id":null,"n":{"$sum":1}}}');
  await page.getByRole('button', { name: 'Run' }).click();
  await page.getByText('OUTPUT').waitFor();
  check((await page.locator('main').innerText()).includes('26'), 'Aggregation $group 输出 26');

  // 14. Export JSONL
  await page.getByRole('button', { name: 'Documents', exact: true }).click();
  await page.getByRole('button', { name: '更多查询操作' }).click();
  await page.getByRole('button', { name: 'Reset' }).click();
  await page.waitForTimeout(300);
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.getByRole('button', { name: 'Export ▾' }).click().then(() =>
      page.getByText('Export JSONL').click(),
    ),
  ]);
  const file = await download.path();
  check(download.suggestedFilename() === 'users.jsonl', 'Export JSONL 下载');
  const exported = await readFile(file, 'utf8');
  check(exported.trim().split('\n').length === 26, '导出行数 = 26');

  // 15. Import File(导入到新集合 users2)
  await page.getByRole('button', { name: 'Add Data ▾' }).click();
  await page.getByRole('button', { name: 'Insert Document', exact: true }).click();
  await page.getByPlaceholder('users').fill('users2');
  await page.locator('div.panel textarea').fill('{"_id":"x1","name":"seed"}');
  await page.getByRole('button', { name: '保存' }).click();
  await page.getByText('users2').first().waitFor();
  await page.getByRole('button', { name: 'Add Data ▾' }).click();
  await page.getByRole('button', { name: 'Import File', exact: true }).click();
  await page.locator('input[type=file]').last().setInputFiles({
    name: 'users.jsonl',
    mimeType: 'application/x-ndjson',
    buffer: Buffer.from(exported),
  });
  await page.getByText('已导入 26 条文档').waitFor();
  check(true, 'Import File(JSONL)');

  // 16. 删除文档(确认弹窗)
  const row = page.locator('table tbody tr').first();
  await row.hover();
  await row.getByRole('button', { name: '删除' }).click();
  await page.getByRole('button', { name: '确认' }).click();
  await page.getByText('文档已删除').waitFor();
  check(true, '删除文档(带确认)');

  // 17. 更多菜单:Options 收纳在 ⋮ 下
  await page.getByRole('button', { name: '更多查询操作' }).click();
  await page.getByRole('button', { name: 'Options ▸' }).click();
  check(await page.getByLabel('Skip').isVisible(), 'Options 收纳进 ⋮ 菜单');
  await page.getByRole('button', { name: '更多查询操作' }).click();

  // 18. 表头固定:容器滚到底后表头仍在容器顶部
  const scroller = page.locator('div.h-full.min-h-0.overflow-auto').first();
  const before = await scroller.locator('thead th').first().boundingBox();
  await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await page.waitForTimeout(200);
  const after = await scroller.locator('thead th').first().boundingBox();
  check(Math.abs((after?.y ?? 0) - (before?.y ?? 0)) < 2, '表头 sticky 固定在滚动容器顶部');
  const scrollBox = await scroller.boundingBox();
  check(
    (scrollBox?.y ?? 0) + (scrollBox?.height ?? 0) <= 900,
    '表格高度限制在视窗内(横向滚动条常驻)',
  );
  await scroller.evaluate((el) => (el.scrollTop = 0));

  // 18b. select 样式化:appearance: base-select + ::picker(select) 渐进增强
  check(
    await page.evaluate(() => CSS.supports('selector(::picker(select))')),
    '浏览器支持 ::picker(select)(否则退回原生外观)',
  );
  await page.getByRole('navigation').getByRole('button', { name: 'Schema' }).click();
  check(
    (await page.locator('select.field').first().evaluate((el) => getComputedStyle(el).appearance)) ===
      'base-select',
    'select 应用 appearance: base-select',
  );
  await page.getByRole('navigation').getByRole('button', { name: 'Documents' }).click();

  // 19. 主题切换:system → light → dark
  const themeBtn = page.getByRole('button', { name: /切换主题/ });
  await themeBtn.click();
  await themeBtn.click();
  check(
    (await page.evaluate(() => document.documentElement.dataset.theme)) === 'dark',
    '主题切换到暗色',
  );
  await page.waitForTimeout(150);
  // 第三次点击回到「跟随系统」:此时系统偏好变化应实时生效
  await themeBtn.click();
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(150);
  check(
    (await page.evaluate(() => document.documentElement.dataset.theme)) === 'dark',
    'system 模式跟随系统暗色',
  );
  await page.emulateMedia({ colorScheme: 'light' });
  await page.waitForTimeout(150);
  check(
    (await page.evaluate(() => document.documentElement.dataset.theme)) === 'light',
    '系统切回亮色时同步',
  );

  if (shot) await page.screenshot({ path: shot, fullPage: true });
  console.log(failures === 0 ? '\nALL PASSED' : `\n${failures} FAILED`);
} finally {
  if (shot) await page.screenshot({ path: shot, fullPage: true }).catch(() => {});
  await browser.close();
}

process.exit(failures === 0 ? 0 : 1);
