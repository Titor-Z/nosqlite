//! 发布元数据一致性:版本号与文档不能各写各的。
//!
//! 真实事故:站点导航与首页硬编码了 `v0.6.11`,升级到 0.7.x 后忘了改,
//! 官网一直显示旧版本。所以这里把「唯一真源」钉死:`Cargo.toml`。

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}: {}", rel, e))
}

/// `Cargo.toml` 里的版本号。
fn cargo_version() -> String {
    let toml = read("Cargo.toml");
    toml.lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .map(str::to_string)
        .expect("Cargo.toml 应有 version 字段")
}

/// `npm/package.json` 必须与 crate 同版本(两者一起发布)。
#[test]
fn npm_version_matches_crate() {
    let pkg = read("npm/package.json");
    let version = cargo_version();
    assert!(
        pkg.contains(&format!("\"version\": \"{}\"", version)),
        "npm/package.json 的 version 应与 Cargo.toml 一致({}),实际:\n{}",
        version,
        pkg.lines().take(5).collect::<Vec<_>>().join("\n")
    );
}

/// CHANGELOG 顶部必须有当前版本的条目。
#[test]
fn changelog_has_current_version() {
    let changelog = read("CHANGELOG.md");
    let version = cargo_version();
    assert!(
        changelog.contains(&format!("## [{}]", version)),
        "CHANGELOG.md 缺少 `## [{}]` 条目 —— 发布前要写更新记录",
        version
    );
}

/// 站点不能再出现硬编码版本号(应从 `Cargo.toml` 读)。
#[test]
fn site_does_not_hardcode_version() {
    for rel in [
        "site/.vitepress/config.mjs",
        "site/.vitepress/theme/components/HomePoster.vue",
    ] {
        let text = read(rel);
        let hardcoded: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter_map(|l| {
                // 形如 'v0.7.2' 的字符串字面量
                let idx = l.find("'v0.")?;
                let rest = &l[idx + 1..];
                Some(&rest[..rest.find('\'').unwrap_or(0).min(rest.len())])
            })
            .collect();
        assert!(
            hardcoded.is_empty(),
            "{} 里硬编码了版本号 {:?} —— 改成读 Cargo.toml,否则升版本必忘",
            rel,
            hardcoded
        );
    }
}

/// `site` 的 config 确实是从 Cargo.toml 取版本的(防止有人改回硬编码)。
#[test]
fn site_reads_version_from_cargo() {
    let config = read("site/.vitepress/config.mjs");
    assert!(
        config.contains("Cargo.toml") && config.contains("version"),
        "site/.vitepress/config.mjs 应从 Cargo.toml 解析版本号"
    );
}
