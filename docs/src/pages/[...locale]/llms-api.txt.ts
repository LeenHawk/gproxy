import type { APIRoute, GetStaticPaths } from 'astro';
import api from '../../../public/generated/openapi.json';
import apiCN from '../../../public/generated/openapi.zh-cn.json';
import apiTW from '../../../public/generated/openapi.zh-tw.json';
import apiRU from '../../../public/generated/openapi.ru.json';
import config from '../../../public/generated/configuration.schema.json';
import configCN from '../../../public/generated/configuration.schema.zh-cn.json';
import configTW from '../../../public/generated/configuration.schema.zh-tw.json';
import configRU from '../../../public/generated/configuration.schema.ru.json';
import channels from '../../../public/generated/channels.json';
import channelsCN from '../../../public/generated/channels.zh-cn.json';
import channelsTW from '../../../public/generated/channels.zh-tw.json';
import channelsRU from '../../../public/generated/channels.ru.json';

export const prerender = true;
export const getStaticPaths: GetStaticPaths = () => [
  { params: { locale: undefined }, props: { lang: 'en' } },
  { params: { locale: 'zh-cn' }, props: { lang: 'zh-cn' } },
  { params: { locale: 'zh-tw' }, props: { lang: 'zh-tw' } },
  { params: { locale: 'ru' }, props: { lang: 'ru' } },
];
const documents = {
  ru: [['HTTP API и структуры запросов и ответов',apiRU],['Схема конфигурации',configRU],['Каталог каналов',channelsRU]],
  en: [['HTTP API and payload schemas',api],['Configuration schema',config],['Channel catalogue',channels]],
  'zh-cn': [['HTTP API 与请求响应结构',apiCN],['配置结构',configCN],['渠道能力目录',channelsCN]],
  'zh-tw': [['HTTP API 與請求回應結構',apiTW],['設定結構',configTW],['通道能力目錄',channelsTW]],
};
export const GET: APIRoute = ({ props }) => new Response([
  '# GPROXY',
  ...documents[props.lang as keyof typeof documents].map(([title,data]) => [
    `## ${title}`, '```json', JSON.stringify(data,null,2), '```',
  ].join('\n\n')),
].join('\n\n')+'\n', { headers: { 'Content-Type':'text/plain; charset=utf-8' } });
