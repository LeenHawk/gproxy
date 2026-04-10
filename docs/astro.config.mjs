// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://gproxy.leenhawk.com',
	integrations: [
		starlight({
			title: {
				en: 'GPROXY Docs',
				'zh-CN': 'GPROXY 文档',
			},
			locales: {
				root: { label: 'English', lang: 'en' },
				zh: { label: '简体中文', lang: 'zh-CN' },
			},
			defaultLocale: 'root',
			customCss: ['./src/styles/custom.css'],
			components: {
				ThemeSelect: './src/components/starlight/ThemeSelect.astro',
			},
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/LeenHawk/gproxy' }],
			sidebar: [
				{
					label: 'Getting Started',
					translations: { 'zh-CN': '入门' },
					items: [
						{ label: 'Overview', translations: { 'zh-CN': '概述' }, slug: 'guides/overview' },
						{ label: 'Quick Start', translations: { 'zh-CN': '快速开始' }, slug: 'guides/getting-started' },
						{ label: 'Architecture', translations: { 'zh-CN': '架构概览' }, slug: 'guides/architecture' },
					],
				},
				{
					label: 'Core Guides',
					translations: { 'zh-CN': '核心指南' },
					items: [
						{ label: 'Configuration', translations: { 'zh-CN': '配置说明' }, slug: 'guides/configuration' },
						{
							label: 'Credential Selection & Cache',
							translations: { 'zh-CN': '凭证选择与缓存亲和' },
							slug: 'guides/credential-selection-cache-affinity',
						},
						{ label: 'Deployment', translations: { 'zh-CN': '部署' }, slug: 'guides/deployment' },
						{
							label: 'Custom Channels',
							translations: { 'zh-CN': '自定义渠道' },
							slug: 'guides/custom-channel-contribution',
						},
						{ label: 'API & Routing', translations: { 'zh-CN': 'API 与路由' }, slug: 'guides/api-routing' },
						{ label: 'Admin & User', translations: { 'zh-CN': '管理端与用户端' }, slug: 'guides/admin-user' },
					],
				},
				{
					label: 'Reference',
					translations: { 'zh-CN': '参考' },
					items: [
						{ label: 'Troubleshooting', translations: { 'zh-CN': '常见问题' }, slug: 'guides/troubleshooting' },
						{ label: 'Development & Testing', translations: { 'zh-CN': '开发与测试' }, slug: 'reference/development' },
					],
				},
			],
		}),
	],
});
