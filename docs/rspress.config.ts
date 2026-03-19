import {defineConfig} from "@rspress/core";
import mermaid from 'rspress-plugin-mermaid';

export default defineConfig({
    root: "content",
    title: "Vetta",
    description:
        "A financial analysis engine for ingesting earnings calls and enabling structured, semantic search across transcripts.",

    icon: "/vetta-logo.png",
    logo: {
        light: "/vetta-logo.png",
        dark: "/vetta-logo.png",
    },
    plugins: [mermaid()],
    themeConfig: {
        socialLinks: [
            {
                icon: "github",
                mode: "link",
                content: "https://github.com/lnivva/vetta",
            },
        ],

        nav: [
            {text: "Guide", link: "/guide/introduction"},
            {text: "Technical Documentation", link: "/technical/architecture"},
            {text: "Configuration", link: "/configuration/"},
        ],

        sidebar: {
            "/guide/": [
                {
                    text: "Getting Started",
                    items: [
                        {text: "Introduction", link: "/guide/introduction"},
                        {text: "Quick Start", link: "/guide/quick-start"},
                    ],
                },
            ],
            "/technical/": [
                {
                    text: "Technical Documentation",
                    items: [
                        {text: "Architecture", link: "/technical/architecture"},
                        {text: "Data Model", link: "/technical/data-model"},
                        {text: "Search & Retrieval", link: "/technical/search-retrieval"},
                    ],
                },
            ],
            "/configuration/": [
                {
                    text: "Configuration",
                    items: [
                        {text: "Overview", link: "/configuration/"},
                        {text: "STT Service", link: "/configuration/stt-service"},
                    ],
                },
            ],
        },

        footer: {
            message: "Vetta Financial Engine",
        },
    },
});