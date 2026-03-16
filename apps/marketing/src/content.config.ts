import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

const roadmap = defineCollection({
  loader: glob({ pattern: "**/*.yaml", base: "./src/content/roadmap" }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    status: z.enum(["shipped", "building", "next"]),
    date: z.string().optional(), // e.g. "Mar 2026", required for shipped
    sortOrder: z.number(), // lower = shown first within status group
  }),
});

const blog = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/blog" }),
  schema: z.object({
    title: z.string(),
    description: z.string(),
    date: z.coerce.date(),
    author: z.string(),
  }),
});

export const collections = { roadmap, blog };
