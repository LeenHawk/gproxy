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
        'Install, configure, and operate GPROXY v3: one gateway in front of many LLM providers, as a native binary, a container, or an edge worker.',
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
      },
      sidebar: [
        {
          label: 'Introduction',
          translations: { 'zh-CN': '介绍' },
          items: [
            {
              label: 'What is GPROXY?',
              slug: 'introduction/what-is-gproxy',
              translations: { 'zh-CN': 'GPROXY 是什么?' },
            },
            {
              label: 'Architecture',
              slug: 'introduction/architecture',
              translations: { 'zh-CN': '架构' },
            },
          ],
        },
        {
          label: 'Getting Started',
          translations: { 'zh-CN': '快速上手' },
          items: [
            {
              label: 'Downloads',
              slug: 'getting-started/downloads',
              translations: { 'zh-CN': '下载' },
            },
            {
              label: 'Installation',
              slug: 'getting-started/installation',
              translations: { 'zh-CN': '安装' },
            },
            {
              label: 'Quick Start',
              slug: 'getting-started/quick-start',
              translations: { 'zh-CN': '快速开始' },
            },
            {
              label: 'First Request',
              slug: 'getting-started/first-request',
              translations: { 'zh-CN': '发送第一个请求' },
            },
          ],
        },
        {
          label: 'Guides',
          translations: { 'zh-CN': '使用指南' },
          items: [
            {
              label: 'Providers & Credentials',
              slug: 'guides/providers',
              translations: { 'zh-CN': 'Provider 与凭证' },
            },
            {
              label: 'Models, Routes & Aliases',
              slug: 'guides/models',
              translations: { 'zh-CN': '模型、路由与别名' },
            },
            {
              label: 'Users & API Keys',
              slug: 'guides/users-and-keys',
              translations: { 'zh-CN': '用户与 API 密钥' },
            },
            {
              label: 'Permissions, Rate Limits & Quotas',
              slug: 'guides/permissions',
              translations: { 'zh-CN': '权限、限流与配额' },
            },
            {
              label: 'Routing Rules & Rule Sets',
              slug: 'guides/rules',
              translations: { 'zh-CN': '路由规则与规则集' },
            },
            {
              label: 'Prompt Caching',
              slug: 'guides/claude-caching',
              translations: { 'zh-CN': '提示缓存' },
            },
            {
              label: 'CLI Clients',
              slug: 'guides/cli-clients',
              translations: { 'zh-CN': 'CLI 客户端' },
            },
            {
              label: 'Console, Portal & Public Site',
              slug: 'guides/console',
              translations: { 'zh-CN': '控制台、门户与公开站点' },
            },
            {
              label: 'Usage, Logs & Audit',
              slug: 'guides/observability',
              translations: { 'zh-CN': '用量、日志与审计' },
            },
            {
              label: 'Adding a Channel',
              slug: 'guides/adding-a-channel',
              translations: { 'zh-CN': '新增通道' },
            },
          ],
        },
        {
          label: 'Reference',
          translations: { 'zh-CN': '参考手册' },
          items: [
            {
              label: 'Configuration',
              slug: 'reference/configuration',
              translations: { 'zh-CN': '配置' },
            },
            {
              label: 'Routing & Endpoints',
              slug: 'reference/routing-table',
              translations: { 'zh-CN': '路由与端点' },
            },
            {
              label: 'Pricing & Tiers',
              slug: 'reference/pricing',
              translations: { 'zh-CN': '价格与分层' },
            },
            {
              label: 'Storage & Cache Backends',
              slug: 'reference/database',
              translations: { 'zh-CN': '存储与缓存后端' },
            },
            {
              label: 'Embedding the Core',
              slug: 'reference/embedding',
              translations: { 'zh-CN': '嵌入核心库' },
            },
          ],
        },
        {
          label: 'Deployment',
          translations: { 'zh-CN': '部署' },
          items: [
            {
              label: 'Building & Releases',
              slug: 'deployment/release-build',
              translations: { 'zh-CN': '构建与发布' },
            },
            {
              label: 'Container',
              slug: 'deployment/docker',
              translations: { 'zh-CN': '容器部署' },
            },
            {
              label: 'Edge Wasm',
              slug: 'deployment/edge',
              translations: { 'zh-CN': 'Edge Wasm' },
            },
            {
              label: 'v2 to v3 Migration',
              slug: 'deployment/v2-to-v3',
              translations: { 'zh-CN': 'v2 到 v3 迁移' },
            },
          ],
        },
      ],
    }),
  ],
});
