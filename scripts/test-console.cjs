const {test}=require('node:test');
const assert=require('node:assert/strict');
const {readFileSync}=require('node:fs');
const {resolve}=require('node:path');
const vm=require('node:vm');

function consoleContext(){
  const context=vm.createContext({});
  const app=readFileSync(resolve(__dirname,'../web/src/app/app.js'),'utf8');
  vm.runInContext(app.slice(0,app.indexOf("document.addEventListener('click'")),context);
  return context;
}

test('历史记录包含日期，跨日运行不会只显示相同时刻',()=>{
  const context=consoleContext();
  const values=vm.runInContext("[time('2026-10-28T08:00:00Z'),time('2026-10-29T08:00:00Z'),time(null)]",context);
  assert.notEqual(values[0],values[1]);
  assert.equal(values[2],'未记录');
});

test('能力列表每项仅有一个状态标签，范围与验证信息完整保留',()=>{
  const context=consoleContext();
  vm.runInContext(`data={capabilities:[{name:'能力 <A>',phase:'P1',status:'partial',scope:'extension',verification:'native-tests-and-browser',detail:'已知限制',source:['crates/harness3-core/src/lib.rs'],target:['future/']}]}`,context);
  const html=vm.runInContext('progressPage()',context);
  assert.equal((html.match(/class="badge /g)||[]).length,1);
  assert(html.includes('后续扩展'));
  assert(html.includes('原生测试与 Web 联调'));
  assert(html.includes('crates/harness3-core/src/lib.rs'));
  assert(html.includes('能力 &lt;A&gt;'));
});

test('调研工具列表按需展开，工具名称不会作为 HTML 执行',()=>{
  const context=consoleContext(),target={};
  context.document={querySelector:()=>target,addEventListener(){}};
  vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/research/research.js'),'utf8'),context);
  vm.runInContext(`data={presets:[{name:'test',version:'1',definition:{protocol:'demo'}}]};presetBindings=()=>[{kind:'process',id:'<img>',manifest:{tools:[{name:'<script>'}]}}];updateResearchCapabilities({elements:{preset:{value:'{"name":"test","version":"1"}'}}})`,context);
  assert(target.innerHTML.includes('<details>'));
  assert(target.innerHTML.includes('可用工具（2）'));
  assert(target.innerHTML.includes('&lt;img&gt;.&lt;script&gt;'));
  assert(!target.innerHTML.includes('<img>'));
});

test('仅操作列固定在表格右侧，数据列正常滚动',()=>{
  const context=consoleContext();
  assert(vm.runInContext("table(['目标','操作'],[])",context).includes('class="table-actions"'));
  assert(!vm.runInContext("table(['目标','时间'],[])",context).includes('class="table-actions"'));
});

test('总览优先显示忙碌实例，不将已终止实例列入未终止列表',()=>{
  const context=consoleContext();
  vm.runInContext(`data={
    instances:[{id:'old',preset_name:'已终止身份',state:'terminated',snapshot:{protocol:'demo'}},
      {id:'ready',preset_name:'就绪身份',state:'ready',snapshot:{protocol:'demo'}},
      {id:'busy',preset_name:'忙碌身份',state:'busy',snapshot:{protocol:'demo'}}],
    goals:[{state:'awaiting-review'}],runs:[],presets:[],models:[],limits:{workers:8}
  }`,context);
  const html=vm.runInContext('overview()',context);
  assert(!html.includes('已终止身份'));
  assert(html.indexOf('忙碌身份')<html.indexOf('就绪身份'));
  assert(html.includes('待验收目标'));
});

test('插件目录只显示内置能力，不提供安装入口，并明确未配置状态',()=>{
  const context=consoleContext();
  vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/plugins/plugins.js'),'utf8'),context);
  vm.runInContext(`data={builtin_plugins:[{id:'web.search',version:'1.0.0',description:'搜索',tools:[]}],builtin_plugin_status:{'web.search':{configured:false}}}`,context);
  const html=vm.runInContext('pluginsPage()',context);
  assert(html.includes('web.search'));assert(html.includes('未配置'));
  assert(!html.includes('安装'));assert(!html.includes('process-form'));assert(!html.includes('plugin-form'));
});

test('详情中的插件授权也服从全局执行开关',()=>{
  const context=consoleContext();
  vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/plugins/plugins.js'),'utf8'),context);
  vm.runInContext(`data={process_plugin_execution:false,presets:[]};
    let detail='';openModal=(title,html)=>{detail=html};
    processDetail({id:'local.test',version:'1',state:'enabled',definition:{manifest:{runtime:{},tools:[]}}});`,context);
  const html=vm.runInContext('detail',context);
  assert(html.includes('全局执行已关闭'));
  assert(!html.includes('允许调用'));
});

test('离开页面或关闭后，过期的产物与文件请求不再打开弹窗',async()=>{
  for(const [file,call] of [['artifacts.js',"artifactDetail('id')"],['workspaces.js',"workspaceOpen('id')"],['workspaces.js',"workspaceFile('id','file.txt')"],['workspaces.js',"workspaceArtifactDetail('id')"]]){
    const context=consoleContext();
    context.document={addEventListener(){}};
    vm.runInContext(readFileSync(resolve(__dirname,'../'+({
      'artifacts.js':'web/src/features/artifacts/artifacts.js',
      'workspaces.js':'web/src/features/workspaces/workspaces.js'
    }[file])),'utf8'),context);
    vm.runInContext(`let finish;api=()=>new Promise(resolve=>{finish=resolve});
      openModal=()=>{throw new Error('stale dialog opened')};`,context);
    const pending=vm.runInContext(call,context);
    vm.runInContext('inspectionRequest++;finish({})',context);
    await pending;
  }
});

test('母体独立定义默认选择可用协议，继承切换恢复必填状态并清除过期错误',()=>{
  const context=consoleContext();
  context.document={addEventListener(){},querySelector(){return null}};
  vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/presets/presets.js'),'utf8'),context);
  vm.runInContext(`
    data={presets:[{name:'parent',version:'1',definition:{purpose:'test',plugin_snapshots:[]}}]};
    const summary={};document.querySelector=()=>summary;
    const field=()=>({required:true,error:'old',setCustomValidity(v){this.error=v}});
    const form={elements:{extends:{value:''},purpose:field(),objective:field(),instructions:field(),protocol:{...field(),value:'',options:[{value:''},{value:'demo'}]}}};
    updatePresetParent(form);
  `,context);
  assert.equal(vm.runInContext('form.elements.protocol.value',context),'demo');
  assert.equal(vm.runInContext('form.elements.protocol.required',context),true);
  vm.runInContext(`form.elements.extends.value=JSON.stringify({name:'parent',version:'1'});updatePresetParent(form)`,context);
  assert.equal(vm.runInContext('form.elements.protocol.value',context),'');
  assert.equal(vm.runInContext('form.elements.purpose.required',context),false);
  assert.equal(vm.runInContext('form.elements.purpose.error',context),'');
  vm.runInContext(`form.elements.extends.value='';updatePresetParent(form)`,context);
  assert.equal(vm.runInContext('form.elements.protocol.value',context),'demo');
  assert.equal(vm.runInContext('form.elements.objective.required',context),true);
});

test('标识格式兼容浏览器 Unicode sets 正则，并拒绝中文和空格',()=>{
  const context=consoleContext();context.document={addEventListener(){},querySelector(){return null}};
  vm.runInContext(readFileSync(resolve(__dirname,'../web/src/features/presets/presets.js'),'utf8'),context);
  vm.runInContext(`data={presets:[],plugins:[],models:[]};let markup='';openModal=(title,html)=>{markup=html};updatePresetParent=()=>{};presetForm()`,context);
  const html=vm.runInContext('markup',context),pattern=html.match(/pattern="([^"]+)"/)[1];
  const re=new RegExp(`^(?:${pattern})$`,'v');
  assert(re.test('researcher-1.0_test'));
  assert(!re.test('研究员'));assert(!re.test('research assistant'));
});
