#!/usr/bin/python3
import json
import os
import sys
import time
manifest = json.load(open('manifest.json'))
mode = open('mode.txt').read()
for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    method = request['method']
    if method == 'initialize':
        result = {'protocol': 'harness3-plugin-rpc/v1', 'id': manifest['id'], 'version': manifest['version'], 'tools': manifest['tools']}
        if mode == 'bad-handshake':
            result['version'] = 'wrong'
    elif method == 'health':
        result = {'healthy': mode != 'unhealthy'}
    elif method == 'shutdown':
        result = {'closed': True}
    elif method == 'tools/call':
        invocation = request['params']['invocation_id']
        print(json.dumps({'jsonrpc': '2.0', 'method': 'progress', 'params': {'invocation_id': invocation, 'message': 'started'}}), flush=True)
        if mode == 'hang':
            time.sleep(60)
        if mode == 'disconnect':
            sys.exit(1)
        if mode == 'oversized':
            print('x' * (128 * 1024 + 1), flush=True)
            continue
        text = request['params']['arguments']['text']
        result = {'content': {'characters': len(text), 'version': manifest['version'], 'home_inherited': 'HOME' in os.environ,
                              'pid': os.getpid()}, 'changed': False}
        if mode == 'invalid-output':
            result['content']['characters'] = 'wrong'
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
    if method == 'shutdown':
        break
