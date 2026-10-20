// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLlmsTxt from 'starlight-llms-txt';

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
  // Installation was merged into the download page.
  redirects: {
    '/getting-started/installation': '/download/',
    '/zh-cn/getting-started/installation': '/zh-cn/download/',
    '/zh-tw/getting-started/installation': '/zh-tw/download/',
    '/ru/getting-started/installation': '/ru/download/',
  },
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
      // Serves /llms.txt, /llms-full.txt and /llms-small.txt from the English pages.
      plugins: [
        starlightLlmsTxt({
          projectName: 'GPROXY',
          details:
            'GPROXY is a Rust LLM gateway: clients speak OpenAI, Anthropic or Gemini wire formats and GPROXY routes them to many upstream providers with per-user keys, quotas, usage accounting and an admin console.',
          exclude: ['legal/**'],
          // Keep generated reference tables valid Markdown in compact sets.
          minify: { whitespace: false },
          customSets: [
            {
              label: 'API and Configuration',
              description: 'Overview of HTTP APIs, configuration and channel capabilities; complete schemas are linked.',
              paths: ['reference/http-api', 'reference/api-schemas', 'reference/configuration-schema', 'reference/channel-catalog'],
            },
          ],
          optionalLinks: [
            { label: 'Полный справочник API и конфигурации на русском', url: 'https://gproxy.leenhawk.com/ru/llms-api.txt' },
            { label: 'Complete API and configuration text', url: 'https://gproxy.leenhawk.com/llms-api.txt' },
            { label: '简体中文完整 API 与配置', url: 'https://gproxy.leenhawk.com/zh-cn/llms-api.txt' },
            { label: '繁體中文完整 API 與設定', url: 'https://gproxy.leenhawk.com/zh-tw/llms-api.txt' },
            { label: 'OpenAPI specification', url: 'https://gproxy.leenhawk.com/generated/openapi.json' },
            { label: 'Configuration JSON Schema', url: 'https://gproxy.leenhawk.com/generated/configuration.schema.json' },
            { label: 'Channel catalogue', url: 'https://gproxy.leenhawk.com/generated/channels.json' },
          ],
        }),
      ],
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/LeenHawk/gproxy' },
      ],
      defaultLocale: 'root',
      locales: {
        root: { label: 'English', lang: 'en' },
        'zh-cn': { label: '简体中文', lang: 'zh-CN' },
        'zh-tw': { label: '繁體中文', lang: 'zh-TW' },
        ru: { label: 'Русский', lang: 'ru' },
      },
      sidebar: [
        {
          label: 'Introduction',
          translations: { 'zh-CN': '介绍', 'zh-TW': '介紹', ru: 'Введение' },
          items: [
            {
              label: 'What is GPROXY?',
              slug: 'introduction/what-is-gproxy',
              translations: { 'zh-CN': 'GPROXY 是什么?', 'zh-TW': 'GPROXY 是什麼?', ru: 'Что такое GPROXY?' },
            },
            {
              label: 'Architecture',
              slug: 'introduction/architecture',
              translations: { 'zh-CN': '架构', 'zh-TW': '架構', ru: 'Архитектура' },
            },
          ],
        },
        {
          label: 'Getting Started',
          translations: { 'zh-CN': '快速上手', 'zh-TW': '快速上手', ru: 'Начало работы' },
          items: [
            {
              label: 'Download & Install',
              slug: 'download',
              translations: { 'zh-CN': '下载与安装', 'zh-TW': '下載與安裝', ru: 'Загрузка и установка' },
            },
            {
              label: 'Quick Start',
              slug: 'getting-started/quick-start',
              translations: { 'zh-CN': '快速开始', 'zh-TW': '快速開始', ru: 'Быстрый старт' },
            },
            {
              label: 'First Request',
              slug: 'getting-started/first-request',
              translations: { 'zh-CN': '发送第一个请求', 'zh-TW': '傳送第一個請求', ru: 'Первый запрос' },
            },
          ],
        },
        {
          label: 'Guides',
          translations: { 'zh-CN': '使用指南', 'zh-TW': '使用指南', ru: 'Руководства' },
          items: [
            {
              label: 'Providers & Credentials',
              slug: 'guides/providers',
              translations: { 'zh-CN': '供应商与凭证', 'zh-TW': '供應商與憑證', ru: 'Провайдеры и учётные данные' },
            },
            {
              label: 'Models & Routes',
              slug: 'guides/models',
              translations: { 'zh-CN': '模型与路由', 'zh-TW': '模型與路由', ru: 'Модели и маршруты' },
            },
            {
              label: 'Users & API Keys',
              slug: 'guides/users-and-keys',
              translations: { 'zh-CN': '用户与 API 密钥', 'zh-TW': '使用者與 API 金鑰', ru: 'Пользователи и ключи API' },
            },
            {
              label: 'Permissions, Rate Limits & Quotas',
              slug: 'guides/permissions',
              translations: { 'zh-CN': '权限、限流与配额', 'zh-TW': '權限、限流與配額', ru: 'Разрешения, лимиты и квоты' },
            },
            {
              label: 'Rewrite Rules & Operation Overrides',
              slug: 'guides/rules',
              translations: { 'zh-CN': '改写规则与操作覆盖', 'zh-TW': '改寫規則與操作覆蓋', ru: 'Перезапись запросов и переопределение операций' },
            },
            {
              label: 'Prompt Caching',
              slug: 'guides/claude-caching',
              translations: { 'zh-CN': '提示缓存', 'zh-TW': '提示快取', ru: 'Кэширование промптов' },
            },
            {
              label: 'CLI Clients',
              slug: 'guides/cli-clients',
              translations: { 'zh-CN': 'CLI 客户端', 'zh-TW': 'CLI 用戶端', ru: 'CLI-клиенты' },
            },
            {
              label: 'Console & Scoped Administration',
              slug: 'guides/console',
              translations: { 'zh-CN': '控制台与范围管理', 'zh-TW': '控制台與範圍管理', ru: 'Консоль и области управления' },
            },
            {
              label: 'Usage, Logs & Audit',
              slug: 'guides/observability',
              translations: { 'zh-CN': '用量、日志与审计', 'zh-TW': '用量、日誌與審計', ru: 'Использование, журналы и аудит' },
            },
            {
              label: 'Adding a Channel',
              slug: 'guides/adding-a-channel',
              translations: { 'zh-CN': '新增通道', 'zh-TW': '新增通道', ru: 'Добавление канала' },
            },
          ],
        },
        {
          label: 'Reference',
          translations: { 'zh-CN': '参考手册', 'zh-TW': '參考手冊', ru: 'Справочник' },
          items: [
            {
              label: 'Configuration',
              slug: 'reference/configuration',
              translations: { 'zh-CN': '配置', 'zh-TW': '配置', ru: 'Конфигурация' },
            },
            {
              label: 'Routing & Endpoints',
              slug: 'reference/routing-table',
              translations: { 'zh-CN': '路由与端点', 'zh-TW': '路由與端點', ru: 'Маршрутизация и конечные точки' },
            },
            {
              label: 'Pricing & Tiers',
              slug: 'reference/pricing',
              translations: { 'zh-CN': '价格与分层', 'zh-TW': '價格與分層', ru: 'Цены и уровни тарификации' },
            },
            {
              label: 'Storage & Cache Backends',
              slug: 'reference/database',
              translations: { 'zh-CN': '存储与缓存后端', 'zh-TW': '儲存與快取後端', ru: 'Хранилища и кэш' },
            },
            {
              label: 'Embedding the Core',
              slug: 'reference/embedding',
              translations: { 'zh-CN': '嵌入核心库', 'zh-TW': '嵌入核心庫', ru: 'Встраивание ядра' },
            },
            {
              label: 'HTTP API Reference',
              slug: 'reference/http-api',
              translations: { 'zh-CN': 'HTTP API 参考', 'zh-TW': 'HTTP API 參考', ru: 'Справочник HTTP API' },
            },
            {
              label: 'API Data Structures',
              slug: 'reference/api-schemas',
              translations: { 'zh-CN': 'API 数据结构', 'zh-TW': 'API 資料結構', ru: 'Структуры данных API' },
            },
            {
              label: 'Configuration Schema',
              slug: 'reference/configuration-schema',
              translations: { 'zh-CN': '配置结构', 'zh-TW': '設定結構', ru: 'Схема конфигурации' },
            },
            {
              label: 'Channel Catalogue',
              slug: 'reference/channel-catalog',
              translations: { 'zh-CN': '渠道能力目录', 'zh-TW': '通道能力目錄', ru: 'Каталог каналов' },
            },
          ],
        },
        {
          label: 'Deployment',
          translations: { 'zh-CN': '部署', 'zh-TW': '部署', ru: 'Развёртывание' },
          items: [
            {
              label: 'Hosted Deployments',
              slug: 'deployment/edge',
              translations: { 'zh-CN': '托管平台部署', 'zh-TW': '託管平台部署', ru: 'Облачные платформы' },
            },
            {
              label: 'Docker & Container Platforms',
              slug: 'deployment/containers',
              translations: { 'zh-CN': 'Docker 与容器平台', 'zh-TW': 'Docker 與容器平台', ru: 'Docker и контейнерные платформы' },
            },
            {
              label: 'Building from Source',
              slug: 'deployment/release-build',
              translations: { 'zh-CN': '从源码构建', 'zh-TW': '從原始碼構建', ru: 'Сборка из исходного кода' },
            },
            {
              label: 'Migrating v2 PostgreSQL to v4',
              slug: 'deployment/v2-postgres-to-v4',
              translations: { 'zh-CN': '从 v2 PostgreSQL 迁移到 v4', 'zh-TW': '從 v2 PostgreSQL 遷移到 v4', ru: 'Миграция PostgreSQL с v2 на v4' },
            },
            {
              label: 'Migrating v3 to v4',
              slug: 'deployment/v3-to-v4',
              translations: { 'zh-CN': '从 v3 迁移到 v4', 'zh-TW': '從 v3 遷移到 v4', ru: 'Миграция с v3 на v4' },
            },
          ],
        },
      ],
    }),
  ],
});
