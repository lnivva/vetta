import {defineConfig} from "@rspress/core";

export default defineConfig({
    root: "content",
    title: "Vetta",
    description: "A financial analysis engine for ingesting earnings calls and enabling structured, semantic search across transcripts.",

    icon: "/vetta-logo.png",
    logo: {
        light: "/vetta-logo.png",
        dark: "/vetta-logo.png",
    },

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
            {text: "Architecture", link: "/architecture/overview"},
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
            "/architecture/": [
                {
                    text: "Architecture",
                    items: [
                        {text: "Overview", link: "/architecture/overview"},
                        {text: "Monorepo Layout", link: "/architecture/monorepo"},
                        {text: "Speech-to-Text Pipeline", link: "/architecture/stt-pipeline"},
                        {text: "STT Strategy Pattern", link: "/architecture/stt-strategy"},
                        {text: "gRPC Transport", link: "/architecture/grpc-transport"},
                        {text: "Python Service", link: "/architecture/python-service"},
                        {text: "Configuration", link: "/architecture/configuration"},
                        {text: "Decision Log", link: "/architecture/decisions"},
                    ],
                },
            ],
        },

        footer: {
            message: "Vetta Financial Engine",
        },
    },
});