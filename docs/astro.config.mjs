// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://gproxy.leenhawk.com',
	integrations: [
		starlight({
			title: 'gproxy',
			description:
				'A high-performance LLM proxy server written in Rust — multi-provider, multi-tenant, with an embedded console.',
			favicon: '/favicon.ico',
			head: [
				{
					tag: 'link',
					attrs: {
						rel: 'icon',
						type: 'image/png',
						sizes: '96x96',
						href: '/favicon-96x96.png',
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'icon',
						type: 'image/svg+xml',
						href: '/favicon.svg',
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'apple-touch-icon',
						sizes: '180x180',
						href: '/apple-touch-icon.png',
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'manifest',
						href: '/site.webmanifest',
					},
				},
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/LeenHawk/gproxy',
				},
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
							label: 'What is gproxy?',
							slug: 'introduction/what-is-gproxy',
							translations: { 'zh-CN': 'gproxy 是什么?' },
						},
						{
							label: 'Architecture',
							slug: 'introduction/architecture',
							translations: { 'zh-CN': '架构概览' },
						},
					],
				},
				{
					label: 'Getting Started',
					translations: { 'zh-CN': '快速上手' },
					items: [
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
							label: 'Providers & Channels',
							slug: 'guides/providers',
							translations: { 'zh-CN': '供应商与通道' },
						},
						{
							label: 'Models & Aliases',
							slug: 'guides/models',
							translations: { 'zh-CN': '模型与别名' },
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
							label: 'Request Rewrite Rules',
							slug: 'guides/rewrite-rules',
							translations: { 'zh-CN': '请求改写规则' },
						},
						{
							label: 'Message Rewrite Rules',
							slug: 'guides/message-rewrite',
							translations: { 'zh-CN': '消息改写规则' },
						},
						{
							label: 'Claude Prompt Caching',
							slug: 'guides/claude-caching',
							translations: { 'zh-CN': 'Claude 提示缓存' },
						},
						{
							label: 'Adding a Channel',
							slug: 'guides/adding-a-channel',
							translations: { 'zh-CN': '新增通道' },
						},
						{
							label: 'Embedded Console',
							slug: 'guides/console',
							translations: { 'zh-CN': '内嵌控制台' },
						},
						{
							label: 'Observability',
							slug: 'guides/observability',
							translations: { 'zh-CN': '可观测性' },
						},
					],
				},
				{
					label: 'Reference',
					translations: { 'zh-CN': '参考手册' },
					items: [
						{
							label: 'Environment Variables',
							slug: 'reference/environment-variables',
							translations: { 'zh-CN': '环境变量' },
						},
						{
							label: 'TOML Config',
							slug: 'reference/toml-config',
							translations: { 'zh-CN': 'TOML 配置' },
						},
						{
							label: 'Dispatch Table',
							slug: 'reference/dispatch-table',
							translations: { 'zh-CN': '路由表' },
						},
						{
							label: 'Database Backends',
							slug: 'reference/database',
							translations: { 'zh-CN': '数据库后端' },
						},
						{
							label: 'Graceful Shutdown',
							slug: 'reference/graceful-shutdown',
							translations: { 'zh-CN': '优雅关机' },
						},
						{
							label: 'Rust SDK',
							slug: 'reference/sdk',
							translations: { 'zh-CN': 'Rust SDK' },
						},
					],
				},
				{
					label: 'Deployment',
					translations: { 'zh-CN': '部署' },
					items: [
						{
							label: 'Release Build',
							slug: 'deployment/release-build',
							translations: { 'zh-CN': '发行版构建' },
						},
						{
							label: 'Docker',
							slug: 'deployment/docker',
							translations: { 'zh-CN': 'Docker 部署' },
						},
					],
				},
				{
					label: 'Downloads',
					translations: { 'zh-CN': '下载' },
					link: '/downloads/',
				},
			],
		}),
	],
});
