import { readFileSync, writeFileSync } from 'node:fs';

const generated = new URL('../public/generated/',import.meta.url);
const files = ['openapi','configuration.schema','channels'];
const normalize = (text) => text.replace(/\s+/g,' ').trim();
for (const locale of ['zh-cn','zh-tw','ru']) {
  const dictionary = JSON.parse(readFileSync(new URL(`../src/i18n/reference/${locale}.json`,import.meta.url),'utf8'));
  const localize = (value) => {
    if (Array.isArray(value)) return value.map(localize);
    if (!value || typeof value !== 'object') return value;
    return Object.fromEntries(Object.entries(value).map(([key,item]) => {
      // These are wire values, not prose, even if an object has a description key.
      if (['default','example','examples','enum','const'].includes(key)) return [key,item];
      if (['description','x-runtime-condition'].includes(key) && typeof item === 'string' && item.trim()) {
        const translated = dictionary[normalize(item)];
        if (!translated) throw new Error(`Missing ${locale} reference translation: ${item}`);
        return [key,translated];
      }
      return [key,localize(item)];
    }));
  };
  for (const file of files) {
    const source=JSON.parse(readFileSync(new URL(`${file}.json`,generated),'utf8'));
    writeFileSync(new URL(`${file}.${locale}.json`,generated),JSON.stringify(localize(source),null,2)+'\n');
  }
}
