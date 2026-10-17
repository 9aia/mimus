import { defineConfig } from "vitepress";
import llmstxt from "vitepress-plugin-llms";

export default defineConfig({
  vite: {
    plugins: [llmstxt()],
  },
  title: "mimus",
  description: "the repo critic — claim verification, docs-as-tests",
  base: "/mimus/",
  ignoreDeadLinks: [/\/api\//],
  themeConfig: {
    nav: [
      { text: "Getting Started", link: "/getting-started" },
      { text: "Tutorial", link: "/tutorials/first-run" },
      { text: "How-to", link: "/how-to/multi-repo" },
      { text: "Explanation", link: "/explanation/architecture" },
      { text: "Reference", link: "/reference/cli" },
      {
        text: "LLMs",
        items: [
          { text: "llms.txt", link: "https://9aia.com/mimus/llms.txt", target: "_blank" },
          { text: "llms-full.txt", link: "https://9aia.com/mimus/llms-full.txt", target: "_blank" },
        ],
      },
    ],
    sidebar: [
      {
        text: "Tutorials",
        items: [{ text: "Your first run", link: "/tutorials/first-run" }],
      },
      {
        text: "How-to",
        items: [
          { text: "Multi-repo", link: "/how-to/multi-repo" },
          { text: "Memory audit", link: "/how-to/kuda-audit" },
          { text: "Authoring rules", link: "/how-to/rules-authoring" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "CLI", link: "/reference/cli" },
          { text: "Config", link: "/reference/config" },
          { text: "Crate API", link: "/reference/crate-api" },
        ],
      },
      {
        text: "Explanation",
        items: [
          { text: "Architecture", link: "/explanation/architecture" },
          { text: "Release model", link: "/explanation/release-model" },
          { text: "vs linters / LLM review", link: "/comparison" },
        ],
      },
    ],
    socialLinks: [{ icon: "github", link: "https://github.com/9aia/mimus" }],
  },
});
