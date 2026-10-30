#!/usr/bin/env python3
"""Render the capability index; --check verifies generated content and source links."""
import json
from pathlib import Path
import sys
root = Path(__file__).resolve().parents[1]
features = json.loads((root / 'docs/implementation-status.json').read_text())
labels = {'implemented':'已实现','partial':'部分实现','planned':'规划中'}
lines = ['# 能力与源码索引', '', '本表与 Web 能力页共享 `docs/implementation-status.json`。能力状态描述已提供的功能与缺口；验证列仅适用于该行已提供的部分。平台概览见[当前平台状态](status.zh-CN.md)，分层与约束见[架构设计](managed-agents.zh-CN.md)。', '', '| 能力 | 状态 | 验证范围 | 当前行为与限制 |', '| --- | --- | --- | --- |']
ids = set()
for feature in features:
    assert feature['id'] not in ids
    ids.add(feature['id'])
    for source in feature['source']:
        assert (root / source).exists(), source
    verified = {'pending':'待验证','not-implemented':'待实现','native-tests-and-browser':'原生测试 + Web 联调'}.get(feature['verification'],'原生测试')
    lines.append(f"| {feature['name']} | {labels[feature['status']]} | {verified} | {feature['detail']} |")
lines += ['', '## 当前模块 → 目标目录', '', '| 能力 | 当前落点 | 目标目录 |', '| --- | --- | --- |']
for feature in features:
    sources = '<br>'.join(f'`{s}`' for s in feature['source']) or '尚无实现模块'
    targets = '<br>'.join(f'`{s}`' for s in feature['target'])
    lines.append(f"| {feature['name']} | {sources} | {targets} |")
lines += ['', '## 当前源码目录', '', '以下从实际文件生成，列出 workspace 各 crate、独立 Web 源码与客户端/插件示例，不包含构建产物。架构设计中的目标树表达职责拆分，不要求预先创建空包。', '', '```text']
def tree(path, prefix=''):
    children = sorted((p for p in path.iterdir() if p.name not in {'__pycache__', '.DS_Store'} and p.suffix != '.pyc'), key=lambda p: (not p.is_dir(), p.name))
    for index, child in enumerate(children):
        last = index == len(children) - 1
        lines.append(prefix + ('└── ' if last else '├── ') + child.name + ('/' if child.is_dir() else ''))
        if child.is_dir():
            tree(child, prefix + ('    ' if last else '│   '))
for directory in ['crates', 'plugins', 'web', 'examples/clients', 'examples/plugins']:
    lines.append(directory + '/')
    tree(root / directory)
lines += ['```', '', '维护方式：修改能力 JSON 或源码目录后运行 `python3 scripts/render-progress.py`；CI 使用 `--check` 检查文档同步与来源路径。', '']
body = '\n'.join(lines)
path = root / 'docs/capabilities.zh-CN.md'
if '--check' in sys.argv:
    if not path.exists() or path.read_text() != body:
        sys.exit('Implementation status document is stale; run scripts/render-progress.py')
else:
    path.write_text(body)
