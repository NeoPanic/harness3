'use strict';
const $ = (s) => document.querySelector(s);
const esc = (value = '') => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const short = s => String(s ?? '').slice(0, 15);
const time = value => value ? new Date(value).toLocaleTimeString('zh-CN', {hour12:false}) : '—';
const labels = {ready:'就绪',busy:'执行中',suspended:'已暂停',terminated:'已终止',queued:'排队中',running:'运行中',completed:'已完成',failed:'失败',interrupted:'中断 · 结果未知',cancelled:'已取消',implemented:'已实现',partial:'部分实现',planned:'规划中'};
const badge = (s, text) => `<span class="badge ${esc(s)}">${esc(text || labels[s] || s)}</span>`;
let data = null, events = [], cursor = 0, authenticated = false, modalContext = null, refreshTask = null;
let toastTimer;

// Repository-native icons: shared geometry, no icon font or network dependency.
const iconPaths = {
  overview:'<path d="m12 3 9 5v8l-9 5-9-5V8Z"/><path d="m3 8 9 5 9-5M12 13v8M7.5 5.5l9 5"/>',
  agents:'<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  presets:'<rect x="5" y="3" width="14" height="18" rx="2"/><path d="M9 8h6M9 12h6M9 16h3"/>',
  runs:'<path d="m9 5 11 7-11 7Z"/><path d="M4 5v14"/>',
  plugins:'<path d="M9 3v5m6-5v5M6 8h12v3a6 6 0 0 1-12 0ZM12 17v4"/>',
  capabilities:'<path d="M9 6h12M9 12h12M9 18h12M3 6l1 1 2-2M3 12l1 1 2-2M3 18h3"/>',
  events:'<path d="M3 12h4l3-8 4 16 3-8h4"/>',
  search:'<circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 5 5"/>',
  arrow:'<path d="M5 12h14m-5-5 5 5-5 5"/>',
  storage:'<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 4 16 4 16 0V5M4 12c0 4 16 4 16 0"/>',
  plus:'<path d="M12 5v14M5 12h14"/>'
};
function icon(name, cls='') { return `<svg class="icon ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${iconPaths[name]||iconPaths.overview}</svg>`; }
const filters = {agents:{query:'',state:'all'},runs:{query:'',state:'all'}};
let renderedPage = '', renderedMarkup = '', composing = false;
function filterBar(kind) {
  const f=filters[kind], states=kind==='agents'?['ready','busy','suspended','terminated']:['queued','running','completed','failed','cancelled','interrupted'];
  return `<div class="toolbar"><label class="search-field">${icon('search')}<input id="directory-search" data-filter="query" type="search" value="${esc(f.query)}" placeholder="${kind==='agents'?'搜索实例 ID、母体或模型协议':'搜索运行 ID、实例或输入'}" aria-label="${kind==='agents'?'搜索实例':'搜索运行'}" autocomplete="off"></label><label class="state-field"><span>状态</span><select id="directory-state" data-filter="state" aria-label="筛选状态"><option value="all">全部状态</option>${states.map(state=>`<option value="${state}" ${f.state===state?'selected':''}>${labels[state]}</option>`).join('')}</select></label><span class="filter-note" role="status">${resultCount(kind)}</span>${f.query||f.state!=='all'?button('清除筛选','clear-filters'):''}</div>`;
}
function resultCount(kind) { const list=kind==='agents'?data.instances:data.runs;return `${filtered(kind,list).length} / ${list.length} 条结果`; }
function filtered(kind,list) { const f=filters[kind],q=f.query.trim().toLocaleLowerCase();return list.filter(item=>(f.state==='all'||item.state===f.state)&&(!q||[item.id,item.preset_name,item.snapshot?.protocol,item.instance_id,item.input].filter(Boolean).join(' ').toLocaleLowerCase().includes(q))); }
function directory(kind) {
  const source=kind==='agents'?data.instances:data.runs,list=filtered(kind,source);
  return list.length?(kind==='agents'?agentsTable(list):runsTable(list)):(source.length?empty('没有符合条件的结果','试试其他关键词，或使用上方的“清除筛选”。'):kind==='agents'?agentsTable([]):runsTable([]));
}
function recentRuns(list) {
  return list.length?`<div class="recent-runs">${list.map(r=>`<button class="recent-run" data-action="run-detail" data-id="${esc(r.id)}"><span class="run-symbol ${esc(r.state)}">${icon('runs')}</span><span class="recent-run-copy"><span class="recent-run-title">${esc(r.input)}</span><span class="recent-run-meta"><span class="mono">${esc(short(r.id))}</span><span>${time(r.started_at||r.created_at)}</span></span></span>${badge(r.state)}<span class="row-arrow">↗</span></button>`).join('')}</div>`:empty('等待第一次执行','从就绪实例提交任务，在这里检查执行证据。',button('查看实例','goto-agents'));
}

function toast(message) { $('#toast').textContent = message; $('#toast').classList.add('visible'); clearTimeout(toastTimer); toastTimer = setTimeout(() => $('#toast').classList.remove('visible'), 4000); }
async function api(path, body, method = 'POST', key = null) {
  const options = {credentials:'same-origin', headers:{}};
  if (body !== undefined) { options.method = method; options.headers['Content-Type'] = 'application/json'; options.body = JSON.stringify(body); }
  if (key) options.headers['Idempotency-Key'] = key;
  let response;
  try { response = await fetch(path, options); }
  catch { throw new Error('无法连接本地平台。请检查服务是否运行，再重试。'); }
  let value;
  try { value = await response.json(); } catch { throw new Error(`HTTP ${response.status}`); }
  if (response.status === 401) { authenticated = false; if (!$('#login-dialog').open) $('#login-dialog').showModal(); }
  if (!response.ok) {
    const code=value.error?.code || value.rejection || `HTTP ${response.status}`;
    const messages={unauthorized:'访问令牌无效或会话已过期，请重新登录。',preset_version_immutable:'这个母体版本已发布，请更换版本号。',plugin_version_immutable:'这个插件版本已安装，请更换版本号。',instance_not_ready:'实例当前无法接收任务，请刷新并选择就绪实例。',instance_quota_exceeded:'实例配额已满，请减少数量或终止闲置实例。',invalid_transition_or_instance_busy:'实例状态已变化或仍在执行，请刷新后重试。',idempotency_conflict:'上次请求的内容已更改，请关闭后重新打开表单提交。',invalid_preset:'请检查名称、版本号和必填内容；文本长度按 UTF-8 字节计算。',invalid_prompt_bundle:'请检查插件 ID、版本号和必填内容。'};
    throw new Error(messages[code] || `操作未完成（${code}），请检查配置后重试。`);
  }
  return value;
}
function page() { return ['overview','agents','presets','runs','plugins','capabilities','events'].includes(location.hash.slice(1)) ? location.hash.slice(1) : 'overview'; }
function head(eyebrow, title, description, buttons = '') { return `<div class="page-head"><div><span class="eyebrow">${eyebrow}</span><h1>${title}</h1><p>${description}</p></div><div class="head-actions">${buttons}</div></div>`; }
function button(label, action, primary = false, extra = '') { return `<button class="button ${primary ? 'primary' : ''}" data-action="${action}" ${extra}>${label}</button>`; }
function empty(title, text, action = '') { return `<div class="empty"><b>${title}</b>${text}${action ? `<br>${action}` : ''}</div>`; }
function panel(title, body, link = '', suffix = '') { return `<section class="panel"><div class="panel-head"><span class="panel-title">${title}${suffix}</span>${link}</div>${body}</section>`; }
function table(headers, rows) { return `<div class="table-wrap" tabindex="0" role="region" aria-label="数据表，可横向滚动"><table><thead><tr>${headers.map(h=>`<th scope="col">${h}</th>`).join('')}</tr></thead><tbody>${rows.join('')}</tbody></table></div>`; }
function agentRows(list) {
  return list.map((a,i) => `<tr><td><div class="table-name"><span class="agent-glyph">${icon('agents')}</span><div><div class="primary-text">${esc(a.preset_name)} <span class="mono">#${esc(a.id.slice(-5))}</span></div><div class="secondary-text mono">${esc(short(a.id))}</div></div></div></td><td>${badge(a.state)}</td><td><span class="mono">${esc(a.preset_version)}</span></td><td><span class="protocol">${esc(a.snapshot.protocol)}</span></td><td>${time(a.created_at)}</td><td><button class="text-button" data-action="agent-detail" data-id="${esc(a.id)}">检查 ↗</button>${a.state === 'ready' ? `<button class="text-button" data-action="run-form" data-id="${esc(a.id)}">运行</button>` : ''}</td></tr>`);
}
function agentsTable(list,compact=false) {
  if(compact && list.length) return '<div class="compact-agents">'+table(['AGENT / 实例','状态','协议','操作'],list.map(a=>`<tr><td><div class="primary-text">${esc(a.preset_name)}</div><div class="secondary-text mono">${esc(short(a.id))}</div></td><td>${badge(a.state)}</td><td class="protocol">${esc(a.snapshot.protocol)}</td><td><button class="text-button" data-action="agent-detail" data-id="${esc(a.id)}">检查 ↗</button></td></tr>`))+'</div>';
  return list.length ? table(['AGENT / 实例','状态','母体版本','模型协议','创建时间','操作'],agentRows(list)) : empty('还没有 Agent 实例','选择一个母体，批量创建你的第一组实例。', button('＋ 创建实例','spawn',true)); }
function runsTable(list) { return list.length ? table(['RUN / 运行','Agent','输入','状态','开始时间','操作'],list.map(r=>`<tr><td class="mono">${esc(short(r.id))}</td><td class="mono">${esc(short(r.instance_id))}</td><td class="run-input" title="${esc(r.input)}">${esc(r.input.slice(0,55))}</td><td>${badge(r.state)}</td><td>${time(r.started_at || r.created_at)}</td><td><button class="text-button" data-action="run-detail" data-id="${esc(r.id)}">检查 ↗</button></td></tr>`)) : empty('尚无执行记录','创建实例并提交一条输入，即可查看工具循环与持久回执。'); }
function activity(list) { return list.length ? `<div class="activity">${list.map(e=>`<div class="activity-item"><span class="event-dot"></span><div><div class="activity-kind mono">${esc(e.kind)}</div><div class="activity-meta mono">${esc(short(e.subject))} · #${e.seq}</div></div><span class="activity-time">${time(e.at)}</span></div>`).join('')}</div>` : empty('暂无事件','操作控制台后，持久事件将出现在这里。'); }
function overview() {
  const active = data.instances.filter(a=>a.state !== 'terminated'), busy = data.instances.filter(a=>a.state === 'busy'), complete = data.runs.filter(r=>r.state === 'completed');
  const implemented = data.capabilities.filter(c=>c.status === 'implemented').length;
  const stats = [
    ['Agent 实例',active.length,'/ 64','agents',`${busy.length} 个执行中 · ${active.length-busy.length} 个待命或暂停`],
    ['已发布母体',data.presets.length,'版本','presets','不可变定义 · 显式版本绑定'],
    ['已完成运行',complete.length,'/ 最近 100 条','runs','持久回执 · 独立 Session'],
    ['已实现能力',implemented,`/ ${data.capabilities.length}`,'capabilities','查看部分实现与后续建设范围']
  ];
  return head('CONTROL PLANE / LOCAL','工作区总览','让 Agent 从定义走向执行。管理实例，追踪每一次决策与回执。',button('查看实现进度','capabilities')+button('＋ 创建实例','spawn',true))+
    `<div class="stats">${stats.map(([label,count,unit,symbol,detail])=>`<div class="stat"><span class="stat-label">${label}</span><span class="stat-icon">${icon(symbol)}</span><div class="stat-number">${count}<small>${unit}</small></div><div class="stat-detail">${detail}</div></div>`).join('')}</div>`+
    `<section class="kernel-map" aria-label="内核运行路径"><div class="kernel-map-head"><span class="eyebrow">THE HARNESS CYCLE</span><span class="kernel-caption">从定义到执行，每一步都有迹可循</span>${badge('good','本地内核')}</div><div class="architecture">${[['presets','01','Preset 母体',`${data.presets.length} 个不可变版本`],['agents','02','Agent 实例',`${active.length} 个独立身份`],['runs','03','Harness 执行','模型 → 工具 → 模型'],['events','04','持久化证据','会话 · 回执 · 事件']].map(([target,num,title,detail],i)=>`${i?`<span class="arch-arrow">${icon('arrow')}</span>`:''}<a class="arch-node" href="#${target}"><span class="arch-number">${num}</span><span class="arch-symbol">${icon(target==='events'?'storage':target)}</span><span><b>${title}</b><small>${detail}</small></span><span class="arch-link">↗</span></a>`).join('')}</div><div class="kernel-map-foot"><span><span class="dot"></span> 单机控制面 · 独立会话</span><span>4 WORKER SLOTS <span class="worker-slots" aria-label="${busy.length} 个实例执行中">${Array.from({length:4},(_,i)=>`<i class="${i<busy.length?'occupied':''}"></i>`).join('')}</span></span></div></section>`+
    `<div class="split"><div>${panel('Agent 实例',agentsTable(data.instances.slice(0,5),true),`<a href="#agents" class="panel-link">全部实例 ${icon('arrow')}</a>`,`<small>${active.length} ACTIVE</small>`)}${panel('最近执行',recentRuns(data.runs.slice(0,4)),`<a href="#runs" class="panel-link">执行记录 ${icon('arrow')}</a>`)}</div><div>${panel('模型连接',`<div class="panel-body">${data.models.map(m=>`<div class="model-row"><span class="model-icon ${m.configured?'configured':''}">${m.protocol==='demo'?'D':icon('plugins')}</span><div><p>${esc(m.protocol)}</p><small>${esc(m.model || '未配置模型')}</small></div>${badge(m.configured?'good':'neutral',m.configured?'已配置':'未配置')}</div>`).join('')}<p class="hint">配置状态反映凭证加载情况，连通结果见执行回执。</p></div>`)}${panel('最近活动',`<div class="panel-body">${activity(events.slice(-5).reverse())}</div>`,`<a href="#events" class="panel-link">事件日志 ${icon('arrow')}</a>`)}</div></div>`;

}
function render() {
  if (!data || composing) return;
  const p=page(), names={overview:'总览',agents:'Agent 实例',presets:'母体 Presets',runs:'执行记录',plugins:'插件与能力',capabilities:'实现进度',events:'事件日志'};
  $('#page-title').textContent=names[p]; $('#nav-count').textContent=data.instances.filter(a=>a.state!=='terminated').length;
  document.querySelectorAll('nav a').forEach(a=>{a.classList.toggle('active',a.dataset.page===p);if(a.dataset.page===p)a.setAttribute('aria-current','page');else a.removeAttribute('aria-current');});
  document.title=`${names[p]} · Harness³`;
  let html='';
  if(p==='overview') html=overview();
  if(p==='agents') html=head('MANAGED INSTANCES','Agent 实例','同一母体可创建多个独立实例；当前支持有限批次，本轮尚无自动协作与扩缩容。',button('＋ 批量创建','spawn',true))+filterBar('agents')+panel('实例目录',`<div id="directory-results">${directory('agents')}</div>`,badge('neutral',`共 ${data.instances.length} 个实例`));
  if(p==='presets') html=head('AGENT DEFINITIONS','母体 Presets','定义用途、目标、指令、模型协议与提示词插件，发布后按版本固定。',button('＋ 发布母体','preset-form',true))+`<div class="cards">${data.presets.map(p=>`<article class="card"><div class="card-top"><span class="agent-glyph">${icon('presets')}</span><span class="mono">${esc(p.version)}</span>${badge('good','已发布')}</div><h3>${esc(p.name)}</h3><p>${esc(p.definition.purpose)}</p><p>${esc(p.definition.objective)}</p><div class="card-bottom"><span class="protocol">${esc(p.definition.protocol)}</span><span><button class="text-button" data-action="preset-detail" data-id="${esc(p.name)}" data-version="${esc(p.version)}">定义 ↗</button><button class="text-button" data-action="spawn" data-id="${esc(p.name)}" data-version="${esc(p.version)}">创建实例</button></span></div></article>`).join('')}</div>`;
  if(p==='runs') html=head('DURABLE EXECUTION','执行记录','展示最近 100 条运行；检查模型请求、工具回执、用量与失败原因。')+filterBar('runs')+panel('运行历史',`<div id="directory-results">${directory('runs')}</div>`,badge('neutral',`${data.runs.length} 条`));
  if(p==='plugins') html=head('CAPABILITIES & BINDINGS','插件与能力','第一版支持内置只读工具与不可变提示词包，新包可在运行时安装并绑定新母体。',button('＋ 安装提示词包','plugin-form',true))+`<div class="callout">外部进程、远程 MCP、代码热替换尚未实现。安装提示词包不会改变已发布母体或既有实例；使用新版本需发布新母体并创建实例。</div><div class="cards"><article class="card"><div class="card-top"><span class="agent-glyph">${icon('plugins')}</span>${badge('implemented','内置工具')}</div><h3>text.inspect</h3><p>统计输入文本的 Unicode 标量、行数和 UTF-8 字节数。只读，不访问文件、不执行代码。</p><div class="card-bottom"><span class="protocol">builtin / v1</span>${badge('good','可调用')}</div></article>${data.plugins.map(p=>`<article class="card"><div class="card-top"><span class="agent-glyph">${icon('presets')}</span>${badge('good','提示词包')}</div><h3>${esc(p.id)}</h3><p>${esc(p.definition.description)}</p><span class="mono wrap">${esc(p.digest)}</span><div class="card-bottom"><span class="protocol">${esc(p.version)}</span><button class="text-button" data-action="plugin-detail" data-id="${esc(p.id)}" data-version="${esc(p.version)}">检查定义 ↗</button></div></article>`).join('')}</div>`;
  if(p==='capabilities') html=head('IMPLEMENTATION MAP','实现进度','状态由当前版本的能力清单提供；部分实现会列出已交付行为与剩余边界。')+`<div class="callout">当前交付：本地内核平台预览。多个实例可以独立运行，组织协作、外部可执行插件和分布式控制面仍在规划中。</div><div class="grid-two">${data.capabilities.map(c=>`<article class="capability"><h3>${esc(c.name)} ${badge(c.status)}</h3><p>${esc(c.detail)}</p></article>`).join('')}</div>`;
  if(p==='events') html=head('PERSISTENT EVENT LOG','事件日志','来自 SQLite 的有序平台事件；包含生命周期与真实执行事件，按游标增量读取。')+panel('平台事件',events.length?table(['序号','事件','对象','时间','载荷'],events.slice(-150).reverse().map(e=>`<tr><td class="mono">#${e.seq}</td><td class="mono">${esc(e.kind)}</td><td class="mono">${esc(short(e.subject))}</td><td>${time(e.at)}</td><td><button class="text-button" data-action="event-detail" data-id="${e.seq}">JSON ↗</button></td></tr>`)):empty('暂无事件','开始操作后查看完整变化。'),badge('neutral',`cursor ${cursor}`))+`<p class="scroll-note">控制台保留最近 500 条已读取事件，列表展示最近 150 条；更早事件可使用 API 的 after 游标查询。</p>`;
  // Do not replace an unchanged view during polling: preserve focus and scroll.
  if(renderedMarkup!==html) {
    const focused=document.activeElement, focusId=focused?.id, action=focused?.dataset.action, itemId=focused?.dataset.id;
    const range=focused instanceof HTMLInputElement && ['search','text'].includes(focused.type)?[focused.selectionStart,focused.selectionEnd]:null;
    const scrolls=[...document.querySelectorAll('.table-wrap')].map(el=>el.scrollLeft);
    $('#content').innerHTML=html;
    document.querySelectorAll('.table-wrap').forEach((el,i)=>{el.scrollLeft=scrolls[i]||0;});
    if(renderedPage===p && !$('#modal').open) {
      const restore=focusId?document.getElementById(focusId):action?[...$('#content').querySelectorAll('[data-action]')].find(el=>el.dataset.action===action&&el.dataset.id===itemId):null;
      if(restore){restore.focus({preventScroll:true});if(range)restore.setSelectionRange(...range);}
    }
  }
  renderedPage=p; renderedMarkup=html;
}
async function refresh(force=false) {
  if(refreshTask){await refreshTask;if(!force)return;}
  refreshTask=readState();
  try {await refreshTask;} finally {refreshTask=null;}
}
async function readState() {
  try {
    data=await api('/api/v1/snapshot'); authenticated=true;
    // Drain a bounded page per tick; the cursor always advances only after a successful read.
    const chunk=await api(`/api/v1/events?after=${cursor}`);
    if(chunk.events.length){events=events.concat(chunk.events).slice(-500);cursor=chunk.events.at(-1).seq;}
    $('#connection').className='badge good'; $('#connection').textContent='● 内核已连接'; $('#notice').textContent='';
    $('#last-update').textContent=`更新于 ${time(Date.now())}`;
    render();
  } catch(error) {
    $('#connection').className='badge failed'; $('#connection').textContent='● 连接未就绪';
    if(authenticated) $('#notice').textContent=`刷新失败：${error.message}。页面保留上次成功读取的状态。`;
  }
}
let modalTrigger=null, inspectionRequest=0;
function openModal(title, html, eyebrow='工作区') {
  inspectionRequest++;
  if(!$('#modal').open&&document.activeElement!==document.body)modalTrigger=document.activeElement;
  $('#modal-error').hidden=true;$('#modal-title').textContent=title;$('#modal-eyebrow').textContent=eyebrow;$('#modal-body').innerHTML=html;
  $('#modal').classList.toggle('inspector',Boolean($('#modal-body .inspector-summary')));
  $('#modal').setAttribute('role',$('#modal [data-form="terminate"]')?'alertdialog':'dialog');
  $('#modal-body').querySelectorAll('input[pattern]').forEach(input=>{input.maxLength=80;input.title='最多 80 个英文字母、数字、点、下划线或连字符';});
  if(!$('#modal').open)$('#modal').showModal();
  const focus=$('#modal form input:not(:disabled), #modal form select:not(:disabled), #modal form textarea')||$('#modal [data-action="close"]');
  focus.focus({preventScroll:true});$('#modal').scrollTop=0;
}
function jsonView(value) { return `<pre tabindex="0" aria-label="JSON 数据">${esc(JSON.stringify(value,null,2))}</pre>`; }
function identity(label,value) { return `<div class="identity"><span>${esc(label)}</span><code>${esc(value)}</code><button class="text-button" data-action="copy" data-value="${esc(value)}" aria-label="复制${esc(label)}">复制</button></div>`; }
function rawView(value,label='原始 JSON') { return `<details class="raw-data"><summary>${label}</summary>${jsonView(value)}</details>`; }
function formEnd(label) { return `<div class="error" id="form-error" role="alert"></div><div class="form-actions">${button('取消','close')}<button class="button primary" type="submit">${label} ↗</button></div>`; }
function spawnForm(name,version) {
  const selected=data.presets.find(p=>p.name===name&&p.version===version)||data.presets.find(p=>p.name==='researcher')||data.presets[0];
  if(!selected) return toast('请先发布一个母体');
  const limit=data.limits.instances,used=data.instances.filter(a=>a.state!=='terminated').length,available=Math.max(0,limit-used);
  openModal('从母体创建实例',`<form data-form="spawn"><p class="form-intro">选择固定版本的母体，创建拥有独立身份与会话的 Agent。</p><label>母体版本<select name="preset">${data.presets.map(p=>`<option value="${esc(JSON.stringify({name:p.name,version:p.version}))}" ${p===selected?'selected':''}>${esc(p.name)} @ ${esc(p.version)} · ${esc(p.definition.protocol)}</option>`).join('')}</select></label><div class="quota-summary"><span>实例配额</span><strong>${used} <span>/ ${limit} 已使用</span></strong><span>还可创建 ${available} 个</span></div><label>实例数量<input name="count" type="number" min="1" max="${Math.max(1,available)}" value="${Math.min(3,available)||1}" ${available?'':'disabled'} required aria-describedby="spawn-hint"></label><p class="hint" id="spawn-hint">${available?'创建后处于就绪状态；提交任务后开始执行。':'当前配额已满。终止闲置实例后可释放配额。'} Worker 最多并发 ${data.limits.workers} 个运行。</p>${formEnd('创建实例')}</form>`,'批量实例化');
  if(!available)$('#modal [type="submit"]').disabled=true;
}
function presetForm() {
  openModal('发布母体版本',`<form data-form="preset"><div class="grid-two"><label>名称<input name="name" required pattern="[A-Za-z0-9._-]+" placeholder="researcher"></label><label>版本<input name="version" required value="1.0.0" pattern="[A-Za-z0-9._-]+"></label></div><label>模型协议<select name="protocol">${data.models.filter(m=>m.configured).map(m=>`<option value="${m.protocol}" ${m.protocol==='openai-responses'?'selected':''}>${esc(m.protocol)} · ${esc(m.model)}</option>`).join('')}</select></label><label>主要功能<input name="purpose" required placeholder="研究给定资料并提出有依据的结论" maxlength="2000"></label><label>预期目标<textarea name="objective" required placeholder="描述 Agent 应达成什么结果" maxlength="8000"></textarea></label><label>系统指令<textarea name="instructions" required maxlength="32000">依据证据回答，区分事实与推测。需要统计文本时使用 text.inspect 工具。</textarea></label><label>提示词插件（可选）<select name="plugin"><option value="">不绑定</option>${data.plugins.map((p,i)=>`<option value="${esc(JSON.stringify({id:p.id,version:p.version}))}">${esc(p.id)} @ ${esc(p.version)}</option>`).join('')}</select></label><p class="hint">发布后内容不可覆盖。修改功能或插件绑定时，请发布新的版本。</p>${formEnd('发布版本')}</form>`,'IMMUTABLE PRESET');
}
function pluginForm() {openModal('安装提示词插件包',`<form data-form="plugin"><div class="grid-two"><label>插件 ID<input name="id" required placeholder="local.research-guidance" pattern="[A-Za-z0-9._-]+"></label><label>版本<input name="version" required value="1.0.0" pattern="[A-Za-z0-9._-]+"></label></div><label>描述<input name="description" required maxlength="2000" placeholder="约束研究输出的结构和证据要求"></label><label>插件指令<textarea name="instructions" required maxlength="32000" placeholder="每项主要结论附上来源，并说明未知信息。"></textarea></label><p class="hint">声明式指令包，不执行代码。可在线安装，按内容摘要保存；需绑定到新发布的母体版本后生效。</p>${formEnd('安装版本')}</form>`,'DECLARATIVE PLUGIN');}
function runForm(id) {
  const a=data.instances.find(item=>item.id===id);if(!a)return;
  const model=data.models.find(m=>m.protocol===a.snapshot.protocol),demo=a.snapshot.protocol==='demo';
  openModal('提交任务',`<form data-form="run" data-id="${esc(id)}"><div class="task-context"><span class="agent-glyph">${icon('agents')}</span><div><strong>${esc(a.preset_name)} <span class="mono">@ ${esc(a.preset_version)}</span></strong><small>${esc(model?.model||a.snapshot.protocol)} · ${esc(a.snapshot.protocol)}</small></div>${badge(a.state)}</div>${identity('实例 ID',id)}<label>任务内容<textarea name="text" required maxlength="64000" rows="6" placeholder="描述需要 Agent 完成的任务…" aria-describedby="run-hint"></textarea></label><p class="hint" id="run-hint">${demo?'此实例使用确定性演示模型，不产生模型费用。':'此任务将调用已配置的真实模型，按供应商规则计费。'} 每次最多 12 轮、24 次工具调用、120 秒。</p>${formEnd('提交任务')}</form>`,'独立会话执行');
}
async function agentDetail(id) {
  const a=data.instances.find(a=>a.id===id); if(!a) return;
  const request=++inspectionRequest;
  const session=await api(`/api/v1/agent-instances/${encodeURIComponent(id)}/session`);
  if(request!==inspectionRequest)return;
  modalContext={type:'agent',id,session};
  const actions=a.state==='ready'?button('提交运行','run-form',true,`data-id="${esc(id)}"`)+button('暂停','suspend',false,`data-id="${esc(id)}"`):a.state==='suspended'?button('恢复','resume',true,`data-id="${esc(id)}"`):'';
  openModal(a.preset_name,`<div class="inspector-summary"><div><span>生命周期</span>${badge(a.state)}</div><div><span>母体版本</span><strong class="mono">${esc(a.preset_version)}</strong></div><div><span>持久会话</span><strong>${session.turns.length} <small>个历史项 · r${session.revision}</small></strong></div></div>${identity('实例 ID',id)}<div class="inspector-actions">${actions}${['ready','suspended'].includes(a.state)?`<button class="text-button danger-action" data-action="terminate" data-id="${esc(id)}">终止实例</button>`:''}</div><div class="tabs"><button data-action="agent-tab" data-tab="snapshot" aria-pressed="true" class="active">配置与目标</button><button data-action="agent-tab" data-tab="session" aria-pressed="false">持久会话</button></div><div id="detail-view">${definitionView(a.snapshot)}</div><details class="raw-data"><summary>实例组与存储信息</summary><dl class="detail-kv"><dt>实例组</dt><dd class="mono">${esc(a.fleet_id)}</dd><dt>检查点</dt><dd>${session.events.length}</dd><dt>模型协议</dt><dd>${esc(a.snapshot.protocol)}</dd></dl></details>`,'Agent 实例');
}
function definitionView(definition) {
  return `<dl class="definition"><dt>主要功能</dt><dd>${esc(definition.purpose)}</dd><dt>预期目标</dt><dd>${esc(definition.objective)}</dd><dt>模型协议</dt><dd class="mono">${esc(definition.protocol)}</dd><dt>系统指令</dt><dd class="instructions">${esc(definition.instructions)}</dd></dl>${rawView(definition,'完整配置 JSON')}`;
}
async function runDetail(id, preserve=false) {
  const r=data.runs.find(r=>r.id===id);if(!r)return;
  const request=++inspectionRequest;
  const selected=preserve&&modalContext?.id===id?modalContext.tab||'output':'output';
  const scroll=preserve?$('#modal').scrollTop:0;
  const session=await api(`/api/v1/agent-instances/${encodeURIComponent(r.instance_id)}/session`);
  if(request!==inspectionRequest)return;
  modalContext={type:'run',id,session,tab:selected};
  const duration=r.report?.execution_ms ?? (r.finished_at&&r.started_at?r.finished_at-r.started_at:null);
  openModal('执行详情',`<div class="inspector-summary"><div><span>运行状态</span>${badge(r.state)}</div><div><span>执行耗时</span><strong>${duration===null?'待结算':`${duration.toLocaleString()} <small>ms</small>`}</strong></div><div><span>模型用量</span><strong>${r.report?.usage?`${r.report.usage.input.toLocaleString()} <small>输入 / ${r.report.usage.output.toLocaleString()} 输出</small>`:'<small>未提供计费用量</small>'}</strong></div></div>${identity('Run ID',id)}<div class="inspector-actions"><span class="hint">详情按需刷新；列表每 1.5 秒同步。</span>${['queued','running'].includes(r.state)?`<button class="button danger" data-action="cancel-run" data-id="${esc(id)}">取消运行</button>`:''}${button('刷新详情','refresh-run',false,`data-id="${esc(id)}"`)}</div><div class="tabs">${[['output','输出'],['report','工具与回执'],['requests','请求快照'],['session','完整会话'],['events','运行事件']].map(([key,label])=>`<button data-action="run-tab" data-tab="${key}" aria-pressed="${key===selected}" class="${key===selected?'active':''}">${label}</button>`).join('')}</div><div id="detail-view">${runTabView(r,selected)}</div><details class="raw-data"><summary>关联实例</summary>${identity('Agent ID',r.instance_id)}</details>`,'运行与证据');
  if(preserve){$('#modal [data-action="refresh-run"]').focus({preventScroll:true});$('#modal').scrollTop=scroll;}
}
function runTabView(r,tab) {
  const requests=modalContext.session.requests.filter(request=>request.run_id===r.report?.run_id);
  if(tab==='output')return runOutput(r);
  if(tab==='report')return reportView(r.report);
  if(tab==='requests'&&!requests.length)return empty('暂无请求快照','模型开始执行后可在此检查请求上下文。');
  return jsonView(tab==='requests'?requests:tab==='session'?modalContext.session:events.filter(e=>e.subject===r.id));
}
function reportView(report) {
  if(!report)return empty('回执尚未生成','运行结算后可以检查工具调用与完整回执。');
  const observations=report.observations||[];
  return `<div class="receipt-summary"><span>${icon('capabilities')} ${observations.length} 次工具调用</span><span class="mono">${report.execution_ms} ms</span></div>${observations.map((item,i)=>`<section class="receipt"><div class="receipt-head"><span class="receipt-index">${String(i+1).padStart(2,'0')}</span><strong class="mono">${esc(item.call.name)}</strong>${badge('neutral',item.output.changed?'有变更':'无变更')}</div><div class="receipt-content"><div><h3>调用参数</h3>${jsonView(item.call.arguments)}</div><div><h3>工具结果</h3>${jsonView(item.output.content)}</div></div></section>`).join('')}${report.error?`<div class="callout">${esc(report.error.code)} · ${esc(report.error.message)}</div>`:''}<details class="raw-data"><summary>查看完整原始回执</summary>${jsonView(report)}</details>`;
}
function runOutput(r) {
  const error=r.report?.error,pending=['queued','running'].includes(r.state);
  const result=error?`<div class="execution-error" role="status"><strong>本次执行未完成</strong><p>${esc(error.message||error.code)}</p><code>${esc(error.code)}</code></div>`:pending?`<div class="execution-pending">${icon('runs')}<div><strong>${r.state==='queued'?'任务已进入队列':'Agent 正在执行'}</strong><p>点击“刷新详情”检查最新结果；关闭面板不会中断任务。</p></div></div>`:`<div class="output">${esc(r.report?.output??(r.state==='cancelled'?'运行已取消，暂无输出。':'此运行未提供文本输出。'))}</div>`;
  return `<div class="output-section"><h3>执行结果</h3>${result}</div><details class="raw-data" ${pending?'open':''}><summary>用户输入</summary><div class="output">${esc(r.input)}</div></details>`;
}
async function handleAction(action, element) {
  const id=element.dataset.id;
  if(action==='copy') {
    try {await navigator.clipboard.writeText(element.dataset.value);element.textContent='已复制';setTimeout(()=>{if(element.isConnected)element.textContent='复制';},1800);}
    catch {throw new Error('浏览器未允许复制，请选中 ID 后手动复制。');}return;
  }
  if(action==='goto-agents') {location.hash='agents';return;}
  if(action==='clear-filters') {filters[page()]={query:'',state:'all'};render();$('#directory-search')?.focus();return;}
  if(action==='close') {if(!$('#modal form[aria-busy="true"]'))$('#modal').close();return;}
  if(action==='logout') {$('#modal').close();await api('/api/logout',{});authenticated=false;data=null;events=[];cursor=0;renderedMarkup='';$('#content').innerHTML='';$('#login-dialog').showModal();return;}
  if(action==='capabilities') {location.hash='capabilities';return;}
  if(action==='spawn') return spawnForm(id,element.dataset.version);
  if(action==='preset-form') return presetForm();
  if(action==='plugin-form') return pluginForm();
  if(action==='run-form') return runForm(id);
  if(action==='agent-detail') return agentDetail(id);
  if(action==='run-detail') return runDetail(id);
  if(action==='refresh-run') {await refresh(true);return runDetail(id,true);}
  if(action==='preset-detail') {const p=data.presets.find(p=>p.name===id&&p.version===element.dataset.version);return openModal(p.name,definitionView(p.definition),`母体版本 ${p.version}`);}
  if(action==='plugin-detail') {const p=data.plugins.find(p=>p.id===id&&p.version===element.dataset.version);return openModal('插件定义',jsonView(p),'PLUGIN / CONTENT DIGEST');}
  if(action==='event-detail') return openModal('事件载荷',jsonView(events.find(e=>String(e.seq)===id)),'PERSISTENT EVENT');
  if(action==='help') return openModal('本地内核预览',`<div class="steps"><div class="step">从母体页发布用途、目标与模型配置。新版本不可覆盖旧定义。</div><div class="step">创建一组独立实例，选择就绪实例提交任务。</div><div class="step">打开执行记录，查看输出、工具回执、请求快照和完整会话。</div><div class="step">在实现进度页确认当前已交付能力与尚未实现的功能。</div></div><p class="hint">所有数据来自同一 SQLite。平台仅监听 loopback，登录使用本地 token；模型密钥仅由服务端读取。</p><div class="form-actions">${button('退出工作区','logout')}</div>`);
  if(['suspend','resume','terminate'].includes(action)) {
    if(action==='terminate')return openModal('终止这个实例？',`<form data-form="terminate" data-id="${esc(id)}"><p class="form-intro">终止后，此实例将无法接收任务，也无法恢复。历史会话与执行回执会继续保留。</p>${identity('实例 ID',id)}${formEnd('确认终止实例')}</form>`,'生命周期操作');
    await api(`/api/v1/agent-instances/${encodeURIComponent(id)}/actions`,{action});$('#modal').close();await refresh();return toast('实例状态已更新');
  }
  if(action==='cancel-run') {await api(`/api/v1/runs/${encodeURIComponent(id)}/cancel`,{});await refresh();await runDetail(id);return toast('取消请求已提交，运行将协作收尾');}
  if(action==='agent-tab') {
    const a=data.instances.find(a=>a.id===modalContext.id);
    $('#detail-view').innerHTML=element.dataset.tab==='session'?jsonView(modalContext.session):definitionView(a.snapshot);
  }
  if(action==='run-tab') {
    const r=data.runs.find(r=>r.id===modalContext.id), tab=element.dataset.tab;
    modalContext.tab=tab;
    $('#detail-view').innerHTML=runTabView(r,tab);
  }
  if(['agent-tab','run-tab'].includes(action)) element.parentElement.querySelectorAll('button').forEach(b=>{b.classList.toggle('active',b===element);b.setAttribute('aria-pressed',String(b===element));});
}
document.addEventListener('click',async event=>{
  const element=event.target.closest('[data-action]');if(!element)return;
  if(element.disabled)return;
  if(!$('#modal').open)modalTrigger=element;
  element.disabled=true;element.setAttribute('aria-busy','true');
  try {await handleAction(element.dataset.action,element);} catch(error){
    if($('#modal').open){$('#modal-error').textContent=error.message;$('#modal-error').hidden=false;}
    else toast(error.message);
  } finally {element.disabled=false;element.removeAttribute('aria-busy');}
});
document.addEventListener('submit',async event=>{
  const form=event.target;
  if(form.id==='login-form') {
    event.preventDefault();
    const submit=form.querySelector('[type="submit"]');submit.disabled=true;submit.textContent='正在连接…';
    try {await api('/api/login',{token:$('#token').value.trim()});$('#token').value='';$('#login-error').textContent='';$('#login-dialog').close();await refresh();}
    catch(error){$('#login-error').textContent=error.message;}
    finally {submit.disabled=false;submit.textContent='进入控制台';}
    return;
  }
  if(!form.dataset.form)return;
  event.preventDefault();if(form.getAttribute('aria-busy')==='true')return;
  const byteLimits={purpose:2000,objective:8000,instructions:32000,description:2000,text:64000};
  for(const [name,limit] of Object.entries(byteLimits)) {
    const field=form.elements.namedItem(name);
    if(field&&new TextEncoder().encode(field.value).length>limit){$('#form-error').textContent=`内容过长：${field.closest('label').firstChild.textContent.trim()}最多 ${limit.toLocaleString()} 个 UTF-8 字节。`;field.focus();return;}
  }
  const values=Object.fromEntries(new FormData(form)),submit=form.querySelector('[type="submit"]'),label=submit.textContent;
  submit.disabled=true;submit.textContent='正在提交…';form.setAttribute('aria-busy','true');$('#form-error').textContent='';
  // Keep the same key when a network failure leaves acceptance uncertain.
  form.dataset.commandKey ||= crypto.randomUUID();
  try {
    let result;
    if(form.dataset.form==='terminate') {result=await api(`/api/v1/agent-instances/${encodeURIComponent(form.dataset.id)}/actions`,{action:'terminate'});}
    if(form.dataset.form==='spawn') {const preset=JSON.parse(values.preset);result=await api('/api/v1/agent-fleets',{preset_name:preset.name,preset_version:preset.version,count:Number(values.count)},'POST',form.dataset.commandKey);location.hash='agents';}
    if(form.dataset.form==='preset') {const plugin=values.plugin===''?null:JSON.parse(values.plugin);result=await api('/api/v1/presets',{name:values.name,version:values.version,purpose:values.purpose,objective:values.objective,instructions:values.instructions,protocol:values.protocol,plugin:plugin?{id:plugin.id,version:plugin.version}:null});location.hash='presets';}
    if(form.dataset.form==='plugin') {result=await api('/api/v1/plugins',values);location.hash='plugins';}
    if(form.dataset.form==='run') {result=await api(`/api/v1/agent-instances/${encodeURIComponent(form.dataset.id)}/runs`,{text:values.text},'POST',form.dataset.commandKey);location.hash='runs';}
    $('#modal').close();await refresh(true);
    if(form.dataset.form==='run'&&data.runs.some(r=>r.id===result.id)){await runDetail(result.id);}
    toast(result?.count ? `已创建 ${result.count} 个独立实例` : form.dataset.form==='run'?'任务已接纳，可在执行详情中检查结果。':form.dataset.form==='terminate'?'实例已终止，历史记录已保留。':'版本已保存');
  }catch(error){const target=form.querySelector('#form-error');if(target){target.textContent=error.message;target.tabIndex=-1;target.focus();}}
  finally {submit.disabled=false;submit.textContent=label;form.removeAttribute('aria-busy');}
});
document.querySelectorAll('nav a').forEach(a=>{a.title=a.textContent.replace(/[◈▦▧▷⌘◫≋0]/g,'').trim();a.querySelector('.nav-icon').innerHTML=icon(a.dataset.page);});
document.addEventListener('compositionstart',()=>{composing=true;});
document.addEventListener('compositionend',()=>{composing=false;});
document.addEventListener('input',event=>{
  const type=event.target.dataset.filter,kind=page();
  if(!type||!filters[kind])return;
  filters[kind][type]=event.target.value;
  $('#directory-results').innerHTML=directory(kind);
  $('.filter-note').textContent=resultCount(kind);
  const clear=$('.toolbar [data-action="clear-filters"]');
  if(clear)clear.hidden=!filters[kind].query&&filters[kind].state==='all';
  else if(filters[kind].query||filters[kind].state!=='all')$('.toolbar').insertAdjacentHTML('beforeend',button('清除筛选','clear-filters'));
});
$('#modal').addEventListener('close',()=>{
  inspectionRequest++;
  if(modalTrigger?.isConnected)modalTrigger.focus({preventScroll:true});
  else if(modalTrigger?.dataset.action){const match=[...document.querySelectorAll('[data-action]')].find(el=>el.dataset.action===modalTrigger.dataset.action&&el.dataset.id===modalTrigger.dataset.id);match?.focus({preventScroll:true});}
});
$('#modal').addEventListener('cancel',event=>{if($('#modal form[aria-busy="true"]'))event.preventDefault();});
$('.skip-link').addEventListener('click',event=>{event.preventDefault();$('#content').focus();});
window.addEventListener('hashchange',()=>{render();$('#content').focus({preventScroll:true});window.scrollTo({top:0,behavior:'instant'});});
refresh();setInterval(()=>{if(authenticated&&!document.hidden)refresh();},1500);
window.addEventListener('focus',()=>{if(authenticated)refresh();});
