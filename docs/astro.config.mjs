// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Markdown renders a soft line break as a space. That is right between Latin
// words and wrong between CJK characters, where the hard-wrapped zh-cn sources
// would otherwise show a stray space at every wrap. Drop the newline only when
// both sides are CJK; a Latin word or code span on either side keeps its space.
const CJK =
  /[⺀-⿿　-〿぀-ヿ㄀-ㄯ㈀-鿿豈-﫿︰-﹏＀-￯\u{20000}-\u{2FFFF}]/u;
const edgeChar = (node, first) => {
  if (!node) return '';
  if (node.type === 'text') return first ? node.value.charAt(0) : node.value.slice(-1);
  if (!node.children?.length) return '';
  return edgeChar(node.children[first ? 0 : node.children.length - 1], first);
};
function joinCjkLines() {
  const walk = (node) => {
    if (!node.children) return;
    node.children.forEach((child, index, siblings) => {
      if (child.type !== 'text') return walk(child);
      child.value = child.value.replace(
        /(.)[ \t]*\n[ \t]*(.)/gsu,
        (match, before, after) => (CJK.test(before) && CJK.test(after) ? before + after : match),
      );
      if (/\n[ \t]*$/.test(child.value)) {
        const before = child.value.replace(/[ \t]*\n[ \t]*$/, '').slice(-1);
        if (CJK.test(before) && CJK.test(edgeChar(siblings[index + 1], true))) {
          child.value = child.value.replace(/[ \t]*\n[ \t]*$/, '');
        }
      }
      if (/^[ \t]*\n/.test(child.value)) {
        const after = child.value.replace(/^[ \t]*\n[ \t]*/, '').charAt(0);
        if (CJK.test(after) && CJK.test(edgeChar(siblings[index - 1], false))) {
          child.value = child.value.replace(/^[ \t]*\n[ \t]*/, '');
        }
      }
    });
  };
  return walk;
}

export default defineConfig({
  site: 'https://gproxy.leenhawk.com',
  markdown: { remarkPlugins: [joinCjkLines] },
  integrations: [
    starlight({
      title: 'GPROXY',
      description:
        'Install, configure and operate GPROXY v4: one gateway in front of many LLM providers, as a native binary, a desktop shell or a Cloudflare Worker.',
      favicon: '/favicon.ico',
      head: [
        {
          tag: 'link',
          attrs: { rel: 'icon', type: 'image/png', sizes: '96x96', href: '/favicon-96x96.png' },
        },
        {
          tag: 'link',
          attrs: { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' },
        },
        {
          tag: 'link',
          attrs: { rel: 'apple-touch-icon', sizes: '180x180', href: '/apple-touch-icon.png' },
        },
        {
          tag: 'link',
          attrs: { rel: 'manifest', href: '/site.webmanifest' },
        },
        {
          // Pagefind's generated locale key is lowercase (`zh-cn`). Normalize
          // the canonical BCP 47 HTML tag before the search bundle initializes
          // so it always selects the matching language index.
          tag: 'script',
          content:
            'document.documentElement.lang = document.documentElement.lang.toLowerCase();',
        },
      ],
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/LeenHawk/gproxy' },
      ],
      defaultLocale: 'root',
      locales: {
        root: { label: 'English', lang: 'en' },
        'zh-cn': { label: '简体中文', lang: 'zh-CN' },
        'zh-tw': { label: '繁體中文', lang: 'zh-TW' },
      },
      sidebar: [
        {
          label: 'Introduction',
          translations: { 'zh-CN': '介绍', 'zh-TW': '介紹' },
          items: [
            {
              label: 'What is GPROXY?',
              slug: 'introduction/what-is-gproxy',
              translations: { 'zh-CN': 'GPROXY 是什么?', 'zh-TW': 'GPROXY 是什麼?' },
            },
            {
              label: 'Architecture',
              slug: 'introduction/architecture',
              translations: { 'zh-CN': '架构', 'zh-TW': '架構' },
            },
          ],
        },
        {
          label: 'Getting Started',
          translations: { 'zh-CN': '快速上手', 'zh-TW': '快速上手' },
          items: [
            {
              label: 'Installation',
              slug: 'getting-started/installation',
              translations: { 'zh-CN': '安装', 'zh-TW': '安裝' },
            },
            {
              label: 'Quick Start',
              slug: 'getting-started/quick-start',
              translations: { 'zh-CN': '快速开始', 'zh-TW': '快速開始' },
            },
            {
              label: 'First Request',
              slug: 'getting-started/first-request',
              translations: { 'zh-CN': '发送第一个请求', 'zh-TW': '傳送第一個請求' },
            },
          ],
        },
        {
          label: 'Guides',
          translations: { 'zh-CN': '使用指南', 'zh-TW': '使用指南' },
          items: [
            {
              label: 'Providers & Credentials',
              slug: 'guides/providers',
              translations: { 'zh-CN': 'Provider 与凭证', 'zh-TW': 'Provider 與憑證' },
            },
            {
              label: 'Models & Routes',
              slug: 'guides/models',
              translations: { 'zh-CN': '模型与路由', 'zh-TW': '模型與路由' },
            },
            {
              label: 'Users & API Keys',
              slug: 'guides/users-and-keys',
              translations: { 'zh-CN': '用户与 API 密钥', 'zh-TW': '使用者與 API 金鑰' },
            },
            {
              label: 'Permissions, Rate Limits & Quotas',
              slug: 'guides/permissions',
              translations: { 'zh-CN': '权限、限流与配额', 'zh-TW': '權限、限流與配額' },
            },
            {
              label: 'Rewrite Rules & Operation Overrides',
              slug: 'guides/rules',
              translations: { 'zh-CN': '改写规则与操作覆盖', 'zh-TW': '改寫規則與操作覆蓋' },
            },
            {
              label: 'Prompt Caching',
              slug: 'guides/claude-caching',
              translations: { 'zh-CN': '提示缓存', 'zh-TW': '提示快取' },
            },
            {
              label: 'CLI Clients',
              slug: 'guides/cli-clients',
              translations: { 'zh-CN': 'CLI 客户端', 'zh-TW': 'CLI 用戶端' },
            },
            {
              label: 'Console & Scoped Administration',
              slug: 'guides/console',
              translations: { 'zh-CN': '控制台与范围管理', 'zh-TW': '控制台與範圍管理' },
            },
            {
              label: 'Usage, Logs & Audit',
              slug: 'guides/observability',
              translations: { 'zh-CN': '用量、日志与审计', 'zh-TW': '用量、日誌與審計' },
            },
            {
              label: 'Adding a Channel',
              slug: 'guides/adding-a-channel',
              translations: { 'zh-CN': '新增通道', 'zh-TW': '新增通道' },
            },
          ],
        },
        {
          label: 'Reference',
          translations: { 'zh-CN': '参考手册', 'zh-TW': '參考手冊' },
          items: [
            {
              label: 'Configuration',
              slug: 'reference/configuration',
              translations: { 'zh-CN': '配置', 'zh-TW': '配置' },
            },
            {
              label: 'Routing & Endpoints',
              slug: 'reference/routing-table',
              translations: { 'zh-CN': '路由与端点', 'zh-TW': '路由與端點' },
            },
            {
              label: 'Pricing & Tiers',
              slug: 'reference/pricing',
              translations: { 'zh-CN': '价格与分层', 'zh-TW': '價格與分層' },
            },
            {
              label: 'Storage & Cache Backends',
              slug: 'reference/database',
              translations: { 'zh-CN': '存储与缓存后端', 'zh-TW': '儲存與快取後端' },
            },
            {
              label: 'Embedding the Core',
              slug: 'reference/embedding',
              translations: { 'zh-CN': '嵌入核心库', 'zh-TW': '嵌入核心庫' },
            },
          ],
        },
        {
          label: 'Deployment',
          translations: { 'zh-CN': '部署', 'zh-TW': '部署' },
          items: [
            {
              label: 'Hosted Deployments',
              slug: 'deployment/edge',
              translations: { 'zh-CN': '托管平台部署', 'zh-TW': '託管平台部署' },
            },
            {
              label: 'Northflank / Render / Heroku',
              slug: 'deployment/containers',
              translations: { 'zh-CN': '容器托管平台', 'zh-TW': '容器託管平台' },
            },
            {
              label: 'Building from Source',
              slug: 'deployment/release-build',
              translations: { 'zh-CN': '从源码构建', 'zh-TW': '從原始碼構建' },
            },
            {
              label: 'Migrating v3 to v4',
              slug: 'deployment/v3-to-v4',
              translations: { 'zh-CN': '从 v3 迁移到 v4', 'zh-TW': '從 v3 遷移到 v4' },
            },
          ],
        },
      ],
    }),
  ],
});
