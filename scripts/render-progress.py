#!/usr/bin/env python3
"""Render the shared roadmap; --check rejects stale documentation or broken source links."""
import json
from pathlib import Path
import sys
root = Path(__file__).resolve().parents[1]
features = json.loads((root / 'docs/implementation-status.json').read_text())
labels = {'implemented':'已实现','partial':'部分实现','planned':'规划中'}
lines = ['# 实现进度与目录落点', '', '本表与 Web 实现进度共享 `docs/implementation-status.json`。状态按具体能力划分，不能据条目数量计算项目完成百分比。`待验证` 表示代码已写入，尚未完成本轮集成验证。', '', '完整目标目录和不变量见 [设计文档](managed-agents.zh-CN.md#12-目标目录结构与模块职责)。当前有根运行内核包、平台包、插件协议包与参考 SDK 包；其余目标目录用于依赖拆分，不要求提前创建空包。', '', '| 阶段 | 能力 | 实现状态 | 验证 | 当前行为 / 缺口 |', '| --- | --- | --- | --- | --- |']
ids = set()
for feature in features:
    assert feature['id'] not in ids
    ids.add(feature['id'])
    for source in feature['source']:
        assert (root / source).exists(), source
    verified = {'pending':'待验证','not-implemented':'待实现','native-tests-and-browser':'原生测试 + Web 联调'}.get(feature['verification'],'既有测试；见预览指南边界')
    lines.append(f"| {feature['phase']} | {feature['name']} | {labels[feature['status']]} | {verified} | {feature['detail']} |")
lines += ['', '## 当前模块 → 目标目录', '', '| 能力 | 当前落点 | 目标目录 |', '| --- | --- | --- |']
for feature in features:
    sources = '<br>'.join(f'`{s}`' for s in feature['source']) or '尚无实现模块'
    targets = '<br>'.join(f'`{s}`' for s in feature['target'])
    lines.append(f"| {feature['name']} | {sources} | {targets} |")
lines += ['', '## 当前源码目录', '', '以下从当前文件生成，供对照第 12 节目标树；只列运行内核、平台和客户端示例，不包含构建产物。', '', '```text']
def tree(path, prefix=''):
    children = sorted(path.iterdir(), key=lambda p: (not p.is_dir(), p.name))
    for index, child in enumerate(children):
        last = index == len(children) - 1
        lines.append(prefix + ('└── ' if last else '├── ') + child.name + ('/' if child.is_dir() else ''))
        if child.is_dir():
            tree(child, prefix + ('    ' if last else '│   '))
for directory in ['src', 'crates/harness3-platform', 'crates/harness3-plugin-protocol', 'crates/harness3-plugin-sdk', 'examples/clients', 'examples/plugins']:
    lines.append(directory + '/')
    tree(root / directory)
lines += ['```', '', '维护方式：修改 JSON 后运行 `python3 scripts/render-progress.py`；CI 使用 `--check` 检查文档同步、当前源码目录和路径存在。验收清单仍以设计文档为准。', '']
body = '\n'.join(lines)
path = root / 'docs/implementation-status.zh-CN.md'
if '--check' in sys.argv:
    if not path.exists() or path.read_text() != body:
        sys.exit('Implementation status document is stale; run scripts/render-progress.py')
else:
    path.write_text(body)
