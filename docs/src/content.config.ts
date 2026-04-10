import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

const baseDocsLoader = docsLoader();

export const collections = {
	docs: defineCollection({
		loader: {
			name: baseDocsLoader.name,
			load: async (context) => {
				context.store.clear();
				return baseDocsLoader.load(context);
			},
		},
		schema: docsSchema(),
	}),
};
