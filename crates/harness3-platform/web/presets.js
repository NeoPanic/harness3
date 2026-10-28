'use strict';
function presetBindings(definition) {return definition.plugin_snapshots || (definition.plugin_snapshot?[definition.plugin_snapshot]:[]);}
function presetForm() {
  const parents=data.presets;
  const plugins=[...(data.builtin_plugins||[]),...data.plugins,...(data.process_plugins||[]).filter(p=>p.state==='enabled')];
  openModal('发布母体版本',`<form data-form="preset"><div class="grid-two"><label>名称<input name="name" required pattern="[A-Za-z0-9._-]+" placeholder="researcher"></label><label>版本<input name="version" required value="1.0.0" pattern="[A-Za-z0-9._-]+"></label></div><label>基于已发布母体<select name="extends"><option value="">独立定义</option>${parents.map(p=>`<option value="${esc(JSON.stringify({name:p.name,version:p.version}))}">${esc(p.name)} @ ${esc(p.version)}</option>`).join('')}</select></label><div id="parent-summary" class="hint"></div><label>模型协议<select name="protocol"><option value="">继承父级协议</option>${data.models.filter(m=>m.configured).map(m=>`<option value="${esc(m.protocol)}" ${m.protocol==='openai-responses'?'selected':''}>${esc(m.protocol)} · ${esc(m.model)}</option>`).join('')}</select></label><label>主要功能<input name="purpose" required placeholder="研究给定资料并提出有依据的结论" maxlength="2000"></label><label>预期目标<textarea name="objective" required placeholder="可引用参数，例如：研究 {{topic}}，所有结论附来源" maxlength="8000"></textarea></label><label>系统指令<textarea name="instructions" required maxlength="32000" placeholder="定义 Agent 的行为和证据要求；留空可继承父级"></textarea></label><fieldset><legend>插件绑定</legend><label>绑定策略<select name="binding_mode"><option value="inherit">沿用父级绑定（独立定义为空）</option><option value="replace">替换为以下选择</option></select></label><div id="preset-bindings">${plugins.length?plugins.map(p=>`<label class="capability-choice"><input type="checkbox" name="binding" value="${esc(JSON.stringify({id:p.id,version:p.version}))}" disabled><span><strong class="mono">${esc(p.id)} @ ${esc(p.version)}</strong><small>${p.kind==='organization'?'内置组织能力 · 同组或活动目标范围':p.definition?.kind==='process'?'外部进程工具':'提示词包'}</small></span></label>`).join(''):'<p class="hint">尚无可绑定插件，可先在插件与能力页安装。</p>'}</div><p class="hint">最多 16 个插件，同一 ID 只能选一个版本。替换并留空可清除继承的全部绑定。</p></fieldset><fieldset><legend>实例参数</legend><label>参数策略<select name="parameter_mode"><option value="inherit">沿用父级 schema 与默认值</option><option value="replace">定义新的参数表</option></select></label><div id="parameter-editor" hidden><div id="parameter-rows"></div><button type="button" class="text-button" data-action="parameter-add">＋ 添加参数</button><p class="hint">用途、目标和系统指令使用 {{参数名}} 引用。默认值留空表示未设置；布尔值填写 true 或 false。参数值作为指令文本传给模型，请勿填写密钥。</p></div></fieldset><p class="hint">发布时固定完整配置、父级来源和能力摘要；最多 16 层单继承。旧实例与会话保持原快照。</p>${formEnd('发布版本')}</form>`,'母体组合');
}
function updatePresetParent(form) {
  const ref=form.elements.extends.value?JSON.parse(form.elements.extends.value):null;
  const parent=ref?data.presets.find(p=>p.name===ref.name&&p.version===ref.version):null;
  for(const field of ['purpose','objective','instructions'])form.elements[field].required=!parent;
  form.elements.protocol.required=!parent;
  if(parent)form.elements.protocol.value='';
  $('#parent-summary').textContent=parent?`继承 ${parent.name} @ ${parent.version}：${parent.definition.purpose}。文本留空沿用父级；${presetBindings(parent.definition).length} 个插件，${Object.keys(parent.definition.parameters_schema?.properties||{}).length} 个参数。`:'独立定义需填写全部文本与模型协议。';
}
function addParameterRow() {
  if(document.querySelectorAll('#parameter-rows .parameter-row').length>=32)return toast('最多 32 个参数');
  const row=document.createElement('div');row.className='parameter-row';
  row.innerHTML=`<div class="grid-two"><label>参数名<input data-param="key" required pattern="[A-Za-z0-9._-]+" maxlength="80" placeholder="topic"></label><label>类型<select data-param="type"><option value="string">文本</option><option value="integer">整数</option><option value="number">数字</option><option value="boolean">布尔值</option></select></label></div><label>默认值（可选）<input data-param="default" placeholder="不填则由创建者提供"></label><div class="parameter-row-actions"><label><input type="checkbox" data-param="required" checked> 创建实例时必填</label><button type="button" class="text-button" data-action="parameter-remove">移除参数</button></div>`;
  $('#parameter-rows').append(row);row.querySelector('input').focus();
}
function typedParameter(value,type) {
  if(type==='string')return value;
  if(type==='boolean'){if(!['true','false'].includes(value))throw new Error('布尔参数只能填写 true 或 false');return value==='true';}
  if(['object','array','null'].includes(type)){const parsed=JSON.parse(value);if(type==='null'&&parsed!==null||type==='array'&&!Array.isArray(parsed)||type==='object'&&(!parsed||Array.isArray(parsed)||typeof parsed!=='object'))throw new Error('参数类型与 schema 不一致');return parsed;}
  const n=Number(value);if(!Number.isFinite(n)||type==='integer'&&!Number.isSafeInteger(n))throw new Error('请填写有效数字；整数须在 JavaScript 安全整数范围内');return n;
}
function presetPayload(form) {
  const f=form.elements,body={name:f.name.value,version:f.version.value};
  if(f.extends.value)body.extends=JSON.parse(f.extends.value);
  for(const key of ['purpose','objective','instructions','protocol'])if(f[key].value.trim())body[key]=f[key].value;
  if(f.binding_mode.value==='replace')body.plugins=Array.from(form.querySelectorAll('[name="binding"]:checked'),e=>JSON.parse(e.value));
  if(f.parameter_mode.value==='replace') {
    const properties={},required=[],defaults={};
    for(const row of form.querySelectorAll('.parameter-row')) {
      const key=row.querySelector('[data-param="key"]').value,type=row.querySelector('[data-param="type"]').value,value=row.querySelector('[data-param="default"]').value;
      if(Object.hasOwn(properties,key))throw new Error(`参数名重复：${key}`);
      Object.defineProperty(properties,key,{value:{type},enumerable:true});
      if(row.querySelector('[data-param="required"]').checked)required.push(key);
      if(value!=='')Object.defineProperty(defaults,key,{value:typedParameter(value,type),enumerable:true});
    }
    body.parameters_schema={type:'object',properties,required,additionalProperties:false};body.parameter_defaults=defaults;
  }
  return body;
}
function scalarControl(schema,value,required=false) {
  const type=schema.type,attrs=`data-value-scalar ${required?'required':''}`;
  if(schema.enum)return `<select ${attrs}><option value="">未设置</option>${schema.enum.map((v,i)=>`<option value="${i}" ${JSON.stringify(v)===JSON.stringify(value)?'selected':''}>${esc(valueText(v))}</option>`).join('')}</select>`;
  if(type==='boolean')return `<select ${attrs}><option value="">未设置</option><option value="true" ${value===true?'selected':''}>是</option><option value="false" ${value===false?'selected':''}>否</option></select>`;
  if(type==='null')return '<span class="hint">空值</span>';
  return `<input ${attrs} ${['integer','number'].includes(type)?`type="number" step="${type==='integer'?'1':'any'}"`:''} value="${esc(value??'')}">`;
}
function structuredEditor(schema,value) {
  const attrs=`data-structured-schema="${esc(JSON.stringify(schema))}"`;
  if(schema.type==='object'&&!schema.enum)return `<div class="structured-editor" ${attrs}>${Object.entries(schema.properties||{}).map(([key,child])=>{
    const required=(schema.required||[]).includes(key),present=value&&Object.hasOwn(value,key);
    return `<div data-member="${esc(key)}"><label>${esc(key)}${required?' · 必填':''}${['object','array'].includes(child.type)&&!child.enum?`<input type="checkbox" data-complex-include ${required||present?'checked':''} ${required?'disabled':''}>` :''}</label>${['object','array'].includes(child.type)&&!child.enum?`<fieldset data-complex-fields ${required||present?'':'hidden disabled'}>${structuredEditor(child,value?.[key])}</fieldset>`:scalarControl(child,value?.[key],required)}${child.description?`<p class="hint">${esc(child.description)}</p>`:''}</div>`;
  }).join('')}</div>`;
  if(schema.type==='array'&&!schema.enum) {
    const values=Array.isArray(value)?value:[];
    return `<div class="structured-editor" ${attrs}><div data-array-items>${values.map(v=>arrayItem(schema.items,v)).join('')}</div><button type="button" class="text-button" data-action="complex-add">添加一项</button><p class="hint">${schema.minItems?`至少 ${schema.minItems} 项；`:''}${schema.maxItems?`最多 ${schema.maxItems} 项`:'按参数规则校验'}</p></div>`;
  }
  return `<div ${attrs}>${scalarControl(schema,value)}</div>`;
}
function arrayItem(schema,value) {return `<div class="structured-item" data-array-item>${structuredEditor(schema,value)}<button type="button" class="text-button" data-action="complex-remove">移除</button></div>`;}
function readScalar(control,schema) {
  if(schema.type==='null')return null;
  const field=control.querySelector('[data-value-scalar]');if(!field||field.value==='')return undefined;
  return schema.enum?schema.enum[Number(field.value)]:typedParameter(field.value,schema.type);
}
function readStructured(editor) {
  const schema=JSON.parse(editor.dataset.structuredSchema);
  if(schema.enum||!['object','array'].includes(schema.type))return readScalar(editor,schema);
  if(schema.type==='array')return [...editor.querySelector('[data-array-items]').children].map(item=>{const value=readStructured(item.querySelector('[data-structured-schema]'));if(value===undefined)throw new Error('列表中有未填写的项目，请补全或移除。');return value;});
  const result={};
  for(const member of editor.children) {
    const key=member.dataset.member;if(!key)continue;const child=schema.properties[key];let value;
    if(['object','array'].includes(child.type)&&!child.enum) {
      if(!member.querySelector('[data-complex-include]').checked)continue;
      value=readStructured(member.querySelector('[data-structured-schema]'));
    }else value=readScalar(member,child);
    if(value!==undefined)Object.defineProperty(result,key,{value,enumerable:true});
  }
  return result;
}
function spawnParameterFields(definition) {
  const schema=definition.parameters_schema;if(!schema)return '<p class="hint">此母体未声明实例参数。</p>';
  const defaults=definition.parameter_defaults||{};
  return `<fieldset><legend>实例参数</legend>${Object.entries(schema.properties).map(([key,s])=>{
    const required=(schema.required||[]).includes(key)&&!Object.hasOwn(defaults,key),def=Object.hasOwn(defaults,key)?`默认：${valueText(defaults[key])}`:'未提供时省略';
    if(['object','array'].includes(s.type)&&!s.enum)return `<div class="complex-parameter" data-instance-complex="${esc(key)}"><label>${esc(key)}${required?' · 必填':''}<input type="checkbox" data-complex-include ${required?'checked disabled':''}> 自定义</label><p class="hint">${esc(def)}</p><fieldset data-complex-fields ${required?'':'hidden disabled'}>${structuredEditor(s,defaults[key])}</fieldset></div>`;
    return `<label data-instance-param="${esc(key)}">${esc(key)}${required?' · 必填':''}${scalarControl(s,undefined,required)}<span class="hint">${esc(def)}${s.description?' · '+esc(s.description):''}</span></label>`;
  }).join('')}<p class="hint">同组实例使用同一组参数；扩容和补建保留参数快照。参数将传给模型，勿填写密钥。</p></fieldset>`;
}
function refreshSpawnParameters(form) {
  const ref=JSON.parse(form.elements.preset.value),definition=data.presets.find(p=>p.name===ref.name&&p.version===ref.version)?.definition;
  $('#spawn-parameters').innerHTML=definition?spawnParameterFields(definition):'';
}
function instanceParameters(form) {
  const ref=JSON.parse(form.elements.preset.value),schema=data.presets.find(p=>p.name===ref.name&&p.version===ref.version)?.definition.parameters_schema;
  const values={};
  for(const container of form.querySelectorAll('[data-instance-param]')) {
    const key=container.dataset.instanceParam,value=readScalar(container,schema.properties[key]);
    if(value!==undefined)Object.defineProperty(values,key,{value,enumerable:true});
  }
  for(const container of form.querySelectorAll('[data-instance-complex]')) {
    if(container.querySelector('[data-complex-include]').checked)Object.defineProperty(values,container.dataset.instanceComplex,{value:readStructured(container.querySelector('[data-structured-schema]')),enumerable:true});
  }
  return values;
}
document.addEventListener('change',event=>{
  const form=event.target.closest('form');if(!form)return;
  if(event.target.matches('[data-complex-include]')) {
    const fields=event.target.closest('[data-member],.complex-parameter').querySelector('[data-complex-fields]');
    fields.hidden=!event.target.checked;fields.disabled=!event.target.checked;
  }
  if(form.dataset.form==='preset') {
    if(event.target.name==='extends')updatePresetParent(form);
    if(event.target.name==='binding_mode')form.querySelectorAll('[name="binding"]').forEach(e=>e.disabled=event.target.value!=='replace');
    if(event.target.name==='parameter_mode') {const editing=event.target.value==='replace';$('#parameter-editor').hidden=!editing;$('#parameter-editor').querySelectorAll('input,select,button').forEach(e=>e.disabled=!editing);}
  }
  if(form.dataset.form==='spawn'&&event.target.name==='preset')refreshSpawnParameters(form);
});
document.addEventListener('click',event=>{
  const button=event.target.closest('[data-action]');if(!button)return;
  if(button.dataset.action==='complex-add') {
    const editor=button.closest('[data-structured-schema]'),schema=JSON.parse(editor.dataset.structuredSchema),items=editor.querySelector('[data-array-items]');
    if(items.children.length>=(schema.maxItems??100))return toast('已达到列表项目上限。');
    items.insertAdjacentHTML('beforeend',arrayItem(schema.items));items.lastElementChild.querySelector('input,select,button')?.focus();
  }
  if(button.dataset.action==='complex-remove')button.closest('[data-array-item]').remove();
  if(button.dataset.action==='parameter-add')addParameterRow();
  if(button.dataset.action==='parameter-remove')button.closest('.parameter-row').remove();
});
