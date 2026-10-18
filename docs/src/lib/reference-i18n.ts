import simplified from '../i18n/reference/zh-cn.json';
import traditional from '../i18n/reference/zh-tw.json';

export type ReferenceLocale = 'en' | 'zh-cn' | 'zh-tw';
const labels = {
  en: {
    method:'Method', path:'Path', dialect:'Dialect', operation:'Operation', transport:'Transport',
    fields:'Fields', field:'Field', type:'Type', required:'Required', default:'Default', description:'Description',
    yes:'Yes', no:'No', none:'None', download:'Download', schema:'JSON Schema', defaults:'Default configuration',
    completeSchema:'Complete JSON Schema', hostConfig:'Shared provider configuration',
    key:'Key', kind:'Kind', capability:'Capability', supported:'Declared support', placeholder:'Example / placeholder',
    login:'Login flows', credentials:'Manual credential kinds', services:'Vendor service routes',
    access:'Access classification', idempotent:'Idempotent', request:'Request body', response:'Responses',
    parameters:'Parameters', location:'Location', status:'Status', section:'Administrative section',
    websocket:'WebSocket messages', clientMessage:'Client message', serverMessage:'Server message', event:'Stream event',
    definitions:'Data structures', references:'Referenced structures', noBody:'No body',
    jsonDownload:'Download JSON', condition:'Availability', optionalBody:'Optional request body',
    Forwarding:'Forwarding', 'Codex provider':'Codex provider', Management:'Management', Portal:'Portal', OAuth:'OAuth', System:'System',
    refresh:'Credential refresh',quotaQuery:'Quota query',quotaReset:'Quota reset',paidUsage:'Paid usage',
    authorization_code:'Authorization code (PKCE)',device_code:'Device code',cookie:'Browser cookie',api_key:'API key',
    string:'String',bool:'Boolean',integer:'Integer',json:'JSON',header_list:'HTTP headers',
    catalog:'Catalogue',identity:'Identity',usage:'Usage',resource:'Resource',settings:'Settings',restricted:'Restricted',telemetry:'Telemetry',refused:'Refused',prefix:'Prefix fallback',
    list:'List',item:'Read / act on an item',create:'Create',delete:'Delete',
  },
  'zh-cn': {
    method:'方法',path:'路径',dialect:'协议',operation:'操作',transport:'传输方式',
    fields:'字段',field:'字段',type:'类型',required:'必填',default:'默认值',description:'说明',
    yes:'是',no:'否',none:'无',download:'下载',schema:'JSON Schema',defaults:'默认配置',
    completeSchema:'完整 JSON Schema',hostConfig:'共用的 Provider 配置',
    key:'配置项',kind:'类型',capability:'能力',supported:'声明支持',placeholder:'示例／占位符',
    login:'登录方式',credentials:'手动添加凭证的类型',services:'厂商服务路由',
    access:'访问分类',idempotent:'幂等',request:'请求体',response:'响应',
    parameters:'参数',location:'位置',status:'状态码',section:'管理权限分区',
    websocket:'WebSocket 消息',clientMessage:'客户端消息',serverMessage:'服务端消息',event:'流式事件',
    definitions:'数据结构',references:'引用的数据结构',noBody:'无正文',
    jsonDownload:'下载 JSON',condition:'可用条件',optionalBody:'请求体可省略',
    Forwarding:'转发 API','Codex provider':'Codex Provider',Management:'管理 API',Portal:'用户门户 API',OAuth:'OAuth',System:'系统 API',
    refresh:'刷新凭证',quotaQuery:'查询额度',quotaReset:'重置额度',paidUsage:'付费用量',
    authorization_code:'授权码（PKCE）',device_code:'设备码',cookie:'浏览器 Cookie',api_key:'API 密钥',
    string:'字符串',bool:'布尔值',integer:'整数',json:'JSON',header_list:'HTTP 请求头',
    catalog:'目录',identity:'身份',usage:'用量',resource:'资源',settings:'设置',restricted:'受限',telemetry:'遥测',refused:'拒绝',prefix:'前缀回退',
    list:'列出',item:'读取／操作单项',create:'创建',delete:'删除',
  },
  'zh-tw': {
    method:'方法',path:'路徑',dialect:'協定',operation:'操作',transport:'傳輸方式',
    fields:'欄位',field:'欄位',type:'型別',required:'必填',default:'預設值',description:'說明',
    yes:'是',no:'否',none:'無',download:'下載',schema:'JSON Schema',defaults:'預設設定',
    completeSchema:'完整 JSON Schema',hostConfig:'共用的 Provider 設定',
    key:'設定項目',kind:'型別',capability:'能力',supported:'宣告支援',placeholder:'範例／預留位置',
    login:'登入方式',credentials:'手動新增憑證的型別',services:'廠商服務路由',
    access:'存取分類',idempotent:'冪等',request:'請求本文',response:'回應',
    parameters:'參數',location:'位置',status:'狀態碼',section:'管理權限分區',
    websocket:'WebSocket 訊息',clientMessage:'用戶端訊息',serverMessage:'伺服器訊息',event:'串流事件',
    definitions:'資料結構',references:'引用的資料結構',noBody:'無本文',
    jsonDownload:'下載 JSON',condition:'可用條件',optionalBody:'請求本文可省略',
    Forwarding:'轉發 API','Codex provider':'Codex Provider',Management:'管理 API',Portal:'使用者入口 API',OAuth:'OAuth',System:'系統 API',
    refresh:'更新憑證',quotaQuery:'查詢額度',quotaReset:'重設額度',paidUsage:'付費用量',
    authorization_code:'授權碼（PKCE）',device_code:'裝置碼',cookie:'瀏覽器 Cookie',api_key:'API 金鑰',
    string:'字串',bool:'布林值',integer:'整數',json:'JSON',header_list:'HTTP 請求標頭',
    catalog:'目錄',identity:'身分',usage:'用量',resource:'資源',settings:'設定',restricted:'受限',telemetry:'遙測',refused:'拒絕',prefix:'前綴回退',
    list:'列出',item:'讀取／操作單項',create:'建立',delete:'刪除',
  },
};
export function referenceText(locale: ReferenceLocale) {
  return labels[locale];
}
export function description(text: string | undefined, locale: ReferenceLocale): string {
  if (!text || locale === 'en') return text ?? '';
  const key = text.replace(/\s+/g,' ').trim();
  const dictionary: Record<string,string> = locale === 'zh-cn' ? simplified : traditional;
  if (!(key in dictionary)) throw new Error(`Missing ${locale} reference translation: ${key}`);
  return dictionary[key];
}
export function localizedSchema(value: any, locale: ReferenceLocale): any {
  if (Array.isArray(value)) return value.map((item) => localizedSchema(item,locale));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key,item]) => [key,
    ['default','example','examples','enum','const'].includes(key) ? item :
      key === 'description' && typeof item === 'string' ? description(item,locale) : localizedSchema(item,locale),
  ]));
  return value;
}
export function schemaReferences(schema: any): string[] {
  const refs = new Set<string>();
  const visit = (value: any) => {
    if (!value || typeof value !== 'object') return;
    if (typeof value.$ref === 'string' && value.$ref.startsWith('#/components/schemas/')) refs.add(value.$ref.slice('#/components/schemas/'.length));
    Object.values(value).forEach(visit);
  };
  visit(schema);
  return [...refs].sort();
}
export function schemaHref(name: string, locale: ReferenceLocale) {
  return `${locale === 'en' ? '' : `/${locale}`}/reference/api-schemas/#${encodeURIComponent(name)}`;
}
