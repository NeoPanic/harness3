'use strict';
let workspaceCatalog=null,workspacePublications=null,workspaceLoadError='',workspaceLoading=false;
async function loadWorkspaces(){
 if(workspaceLoading)return;workspaceLoading=true;workspaceLoadError='';
 try{[workspaceCatalog,workspacePublications]=await Promise.all([api('/api/v1/workspaces'),api('/api/v1/workspace-artifacts')]);}
 catch(error){workspaceLoadError=error.message;}
 finally{workspaceLoading=false;if(page()==='workspaces')render();}
}
function workspacePage(){
 if(!workspaceCatalog&&!workspaceLoading&&!workspaceLoadError)queueMicrotask(loadWorkspaces);
 const rows=workspaceCatalog?.instances||[],artifacts=workspacePublications?.artifacts||[];
 return head('FILES & EVIDENCE','文件工作区','每个 Agent 保存自己的文件，通过发布不可变快照交付给同目标的其他 Agent。',button(workspaceLoading?'正在刷新…':'刷新文件与产物','workspace-refresh',false,workspaceLoading?'disabled':''))+
 (workspaceLoadError?`<div class="callout" role="alert">${esc(workspaceLoadError)}。${button('重试','workspace-refresh')}</div>`:'')+
 (workspaceCatalog&&!workspaceCatalog.configured?'<div class="callout">文件工作区尚未配置。重启本地服务后使用数据目录下的 workspaces 文件夹。</div>':'')+
 panel('共享文件与 JSON 产物',artifacts.length?table(['名称 / 格式','来源','状态 / 大小','操作'],artifacts.map(a=>`<tr><td><div class="primary-text wrap">${esc(a.name)}</div><div class="secondary-text">${a.kind==='json'?'结构化 JSON':'文件快照'}${a.path?` · ${esc(a.path)}`:''}</div></td><td class="mono wrap">${esc(short(a.instance_id))}<div class="secondary-text">${a.goal_id?`<a href="#goal/${esc(a.goal_id)}">${esc(short(a.goal_id))} ↗</a>`:'独立运行'}</div></td><td>${badge(a.source_state)}<div class="secondary-text">${a.bytes.toLocaleString()} B</div></td><td>${button('查看快照','workspace-artifact',false,`data-id="${esc(a.id)}"`)}</td></tr>`)):empty('尚无共享文件产物','Agent 使用 platform.workspace.publish 发布文件或 JSON，同目标成员可读取对应快照。'))+
 panel('实例工作区',!workspaceCatalog?'<div class="loading">正在读取实例文件目录…</div>':rows.length?table(['Agent / 母体','运行状态','已发布快照','操作'],rows.map(a=>`<tr><td><div class="primary-text">${esc(a.preset_name)}</div><div class="secondary-text mono">${esc(short(a.id))}</div></td><td>${badge(a.state)}</td><td>${a.artifact_count}</td><td>${button('浏览文件','workspace-open',false,`data-id="${esc(a.id)}"`)}</td></tr>`)):empty('还没有实例','从母体创建 Agent 后，绑定 platform.workspace 即可使用文件工具。'))+
 '<p class="hint">当前支持 UTF-8 文本和 JSON，每份最多 1 MiB。文件目录按需刷新；产物目录显示最近 200 份快照。来源运行未成功的快照仍是临时证据，不能据此声称事实已验证。</p>';
}
async function workspaceOpen(id,path=''){
 const listing=await api(`/api/v1/agent-instances/${encodeURIComponent(id)}/workspace?path=${encodeURIComponent(path)}`);
 const parent=path.split('/').filter(Boolean).slice(0,-1).join('/');
 openModal(path||'实例文件工作区',`<p class="hint"><span class="mono">${esc(id)}</span> · 相对路径 <span class="mono">${esc(path||'/')}</span></p><div class="inspector-actions">${path?button('上一级','workspace-directory',false,`data-id="${esc(id)}" data-path="${esc(parent)}"`):''}${button('刷新目录','workspace-directory',false,`data-id="${esc(id)}" data-path="${esc(path)}"`)}</div>${listing.entries.length?table(['名称','类型','大小','操作'],listing.entries.map(e=>`<tr><td class="wrap">${esc(e.name)}</td><td>${({directory:'文件夹',file:'文件',symlink:'符号链接'})[e.kind]||esc(e.kind)}</td><td>${e.kind==='directory'?'—':`${e.bytes.toLocaleString()} B`}</td><td>${e.kind==='directory'?button('打开文件夹','workspace-directory',false,`data-id="${esc(id)}" data-path="${esc(e.path)}"`):e.readable?button('查看文件','workspace-file',false,`data-id="${esc(id)}" data-path="${esc(e.path)}"`):'<span class="hint">无法预览</span>'}</td></tr>`)):empty('此目录尚无文件','绑定 platform.workspace 的 Agent 可以创建文件和子目录。')}${listing.truncated?'<p class="hint">仅显示前 200 项，请进入具体子目录查看。</p>':''}`,'私有实例文件');
}
async function workspaceFile(id,path){
 const file=await api(`/api/v1/agent-instances/${encodeURIComponent(id)}/workspace/file?path=${encodeURIComponent(path)}`);
 openModal(path,`<p class="hint">当前文件内容 · ${file.bytes.toLocaleString()} B · 尚未发布为共享快照</p><div class="output">${esc(file.content)}</div><details class="raw-data"><summary>来源与内容摘要</summary>${identity('Agent ID',id)}${identity('Path',path)}${identity('SHA-256',file.digest)}</details>`,'实例文件');
}
async function workspaceArtifactDetail(id){
 const a=await api(`/api/v1/workspace-artifacts/${encodeURIComponent(id)}`);
 openModal(a.name,`<div class="inspector-summary"><div><span>内容格式</span><strong>${a.kind==='json'?'结构化 JSON':'文件快照'}</strong></div><div><span>完整性</span><strong>${a.integrity_verified?'摘要匹配':'摘要不匹配'}</strong></div><div><span>大小</span><strong>${a.bytes.toLocaleString()} B</strong></div></div><p class="hint">来源 ${badge(a.source_state)} · 发布时保存原文，之后修改实例文件不会改变此快照。内容不作为 HTML 执行。</p><div class="inspector-actions">${button('检查来源 Run','run-detail',false,`data-id="${esc(a.run_id)}"`)}${button('下载原文','workspace-download',false,`data-id="${esc(id)}"`)}</div><div class="output">${esc(a.content)}</div><details class="raw-data"><summary>来源与内容摘要</summary>${identity('Artifact ID',a.id)}${identity('Agent ID',a.instance_id)}${identity('Run ID',a.run_id)}${identity('Goal ID',a.goal_id||'独立运行')}${identity('SHA-256',a.digest)}</details>`,'不可变共享文件');
}
async function workspaceAction(action,element){
 const id=element.dataset.id;if(action==='workspace-refresh'){await loadWorkspaces();return true;}
 if(action==='workspace-open'||action==='workspace-directory'){await workspaceOpen(id,element.dataset.path||'');return true;}
 if(action==='workspace-file'){await workspaceFile(id,element.dataset.path);return true;}
 if(action==='workspace-artifact'){await workspaceArtifactDetail(id);return true;}
 if(action==='workspace-download'){
  const a=await api(`/api/v1/workspace-artifacts/${encodeURIComponent(id)}`);if(!a.integrity_verified)throw new Error('内容摘要不匹配，已停止下载。');
  const url=URL.createObjectURL(new Blob([a.content],{type:a.media_type})),link=document.createElement('a');link.href=url;link.download=a.kind==='json'?`${a.name}.json`:a.path?.split('/').pop()||`${a.name}.txt`;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);return true;
 }
 return false;
}
