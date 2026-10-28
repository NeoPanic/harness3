'use strict';
function pluginsPage() {
  const process=data.process_plugins||[],enabled=data.process_plugin_execution;
  const rows=process.map(p=>`<tr><td><div class="primary-text">${esc(p.id)}</div><div class="secondary-text mono">${esc(p.version)}</div></td><td>${badge(p.state==='enabled'?'good':'neutral',p.state==='enabled'?'允许调用':'已禁用')}</td><td>${p.definition.manifest.tools.length}</td><td>${p.health_error?badge('failed','检查失败'):badge('good','已通过')}<div class="secondary-text">${time(p.health_at)}</div></td><td><button class="text-button" data-action="process-detail" data-id="${esc(p.id)}" data-version="${esc(p.version)}">检查</button><button class="text-button" data-action="process-check" data-id="${esc(p.id)}" data-version="${esc(p.version)}" ${enabled?'':'disabled'}>检查健康</button><button class="text-button" data-action="process-state" data-id="${esc(p.id)}" data-version="${esc(p.version)}" ${enabled?'':'disabled'}>${p.state==='enabled'?'禁用':'启用'}</button></td></tr>`);
  return head('','插件与能力','按版本安装能力，在母体发布时固定绑定。已发布母体与既有实例保留自己的快照。',button('安装提示词包','plugin-form')+button('安装进程插件','process-form',true,enabled?'':'disabled'))+
    `<div class="callout">${enabled?'进程插件已由本地操作者显式启用，以 daemon 的 OS 用户权限执行。平台不向它传递模型密钥或访问 token，当前没有工作区/网络沙箱。':'进程执行未启用。启动 daemon 时追加 --allow-process-plugins，再安装你信任的本地包。已安装版本和历史绑定仍可检查。'}</div>`+
    panel('外部进程版本',rows.length?table(['插件 / 版本','调用状态','工具','最近健康检查','操作'],rows):empty('尚未安装进程插件','示例位于 examples/plugins/text-inspector。安装校验与健康检查通过后，可绑定到新的母体版本。'),badge('neutral',`${process.length} 个版本`))+
    panel('声明式提示词包',data.plugins.length?table(['插件 / 版本','说明','操作'],data.plugins.map(p=>`<tr><td><div class="primary-text">${esc(p.id)}</div><div class="secondary-text mono">${esc(p.version)}</div></td><td>${esc(p.definition.description)}</td><td><button class="text-button" data-action="plugin-detail" data-id="${esc(p.id)}" data-version="${esc(p.version)}">检查定义</button></td></tr>`)):empty('尚未安装提示词包','提示词包只贡献指令，不执行代码。'))+
    panel('内置工具',`<div class="panel-body"><strong class="mono">text.inspect</strong><p class="hint">统计 Unicode 标量、行数与 UTF-8 字节，不访问文件。外部工具以插件 ID 为命名空间，与内置工具一起进入 Harness 循环。</p></div>`)+
    `<p class="hint">新旧插件版本可同时绑定给不同母体。安装新版本无需重启 daemon；切换或回滚请发布引用目标版本的新母体并创建新实例。现有会话不会被静默改写。远程 MCP loader、自动 rollout 和 OS 沙箱仍待实现。</p>`;
}
function processInstallForm() {
  openModal('安装本地进程插件',`<form data-form="process-install"><p class="form-intro">平台读取包目录中的 manifest.json，复制并校验声明文件，再启动入口做握手与健康检查。安装通过后才能绑定母体。</p><label>包目录的绝对路径<input name="source_dir" required maxlength="2048" placeholder="/absolute/path/to/plugin-package" autocomplete="off" spellcheck="false"></label><p class="hint">只安装你信任的代码。该进程拥有 daemon 用户的本机权限，安装检查本身就会执行入口。包内禁止符号链接与目录逃逸，最多 32 个文件、合计 8 MiB。</p>${formEnd('校验并安装')}</form>`,'进程插件安装');
}
function processDetail(p) {
  const manifest=p.definition.manifest;
  const bound=data.presets.filter(item=>presetBindings(item.definition).some(b=>b.kind==='process'&&b.id===p.id&&b.version===p.version));
  openModal(`${p.id} @ ${p.version}`,`<div class="inspector-summary"><div><span>调用状态</span>${badge(p.state==='enabled'?'good':'neutral',p.state==='enabled'?'允许调用':'已禁用')}</div><div><span>健康检查</span><strong>${p.health_error?'失败':'已通过'}</strong></div><div><span>当前列表中的母体绑定</span><strong>${bound.length}</strong></div></div><p class="form-intro">${esc(manifest.description)}</p>${identity('包摘要',p.digest)}${p.health_error?`<div class="callout">${esc(p.health_error)}。检查包缓存和入口，健康检查通过后再启用。</div>`:''}<dl class="detail-kv"><dt>协议</dt><dd class="mono">${esc(manifest.runtime.protocol)}</dd><dt>包内入口</dt><dd class="mono">${esc(manifest.runtime.entry)}</dd><dt>调用期限</dt><dd>${manifest.timeout_ms} ms；取消后最多额外等待 250 ms 回执</dd><dt>指令</dt><dd class="instructions">${esc(manifest.instructions)}</dd><dt>母体绑定</dt><dd>${bound.map(b=>`${esc(b.name)} @ ${esc(b.version)}`).join('<br>')||'当前列表未发现绑定'}</dd></dl>${panel('工具契约',table(['工具','副作用','输入 / 输出'],manifest.tools.map(t=>`<tr><td><strong class="mono">${esc(p.id)}.${esc(t.name)}</strong><p class="hint">${esc(t.description)}</p></td><td>${t.mutates?'声明有副作用':'声明只读'}<div class="secondary-text">${t.parallel_safe?'允许安全并行':'串行边界'}</div></td><td><details><summary>输入 schema</summary>${jsonView(t.input_schema)}</details><details><summary>输出 schema</summary>${jsonView(t.output_schema)}</details></td></tr>`)))}${rawView(p,'完整版本、文件摘要与检查记录')}`,'进程插件版本');
}
async function processAction(action,element) {
  if(action==='process-form'){processInstallForm();return true;}
  if(!['process-detail','process-check','process-state'].includes(action))return false;
  const p=(data.process_plugins||[]).find(p=>p.id===element.dataset.id&&p.version===element.dataset.version);
  if(!p)throw new Error('版本不在当前列表中，请刷新后重试。');
  if(action==='process-detail'){processDetail(p);return true;}
  if(action==='process-state') {
    const disable=p.state==='enabled';
    openModal(disable?'禁用这个版本？':'启用这个版本？',`<form data-form="process-state" data-id="${esc(p.id)}" data-version="${esc(p.version)}" data-state="${disable?'disable':'enable'}"><p class="form-intro">${disable?'立即阻止所有已绑定实例的新调用，并请求取消活动调用。已经发生的外部效果不会撤销；中断回执可能标记结果未知。':'先重新校验包摘要和进程健康，再允许已绑定实例调用。旧版本绑定与会话保持不变。'}</p>${identity('插件版本',`${p.id} @ ${p.version}`)}${formEnd(disable?'确认禁用':'检查并启用')}</form>`,'插件调用授权');return true;
  }
  await api(`/api/v1/process-plugins/${encodeURIComponent(p.id)}/${encodeURIComponent(p.version)}/actions`,{action:'check'});await refresh();toast('健康检查通过');return true;
}
