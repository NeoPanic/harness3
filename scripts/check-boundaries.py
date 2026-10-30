#!/usr/bin/env python3
"""Check workspace dependency boundaries, including transitive normal dependencies."""
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=root,
))
packages = {p['name']: p for p in metadata['packages']}
graph = {name: {d['name'] for d in p['dependencies'] if d['kind'] != 'dev'}
         for name, p in packages.items()}

def closure(name, trail=()):
    assert name not in trail, f'normal dependency cycle: {trail + (name,)}'
    result = set(graph.get(name, ()))
    for child in graph.get(name, ()):
        if child in graph:
            result.update(closure(child, trail + (name,)))
    return result

for name in graph:
    closure(name)
for name in ['harness3-domain', 'harness3-client', 'harness3-models', 'harness3-plugins']:
    dependencies = closure(name)
    assert not dependencies & {'rusqlite', 'axum', 'harness3-platform', 'harness3-worker', 'harness3-server'}, (name, dependencies)
assert not closure('harness3-client') & {'harness3', 'harness3-plugins', 'harness3-models'}
assert not closure('harness3-domain') & {'harness3', 'reqwest', 'tokio'}
assert not closure('harness3-platform') & {'harness3-client', 'harness3-worker', 'harness3-server', 'harness3-cli'}
assert not closure('harness3-server') & {'harness3-cli', 'harness3-client'}
assert any(t['name'] == 'harness3' and 'bin' in t['kind']
           for t in packages['harness3-cli']['targets'])
print('Workspace dependency boundaries PASS')
