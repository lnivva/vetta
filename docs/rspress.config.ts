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
                {
                    text: "Installation",
                    items: [
                        {text: "Prerequisites", link: "/guide/installation/prerequisites"},
                        {text: "macOS", link: "/guide/installation/macos"},
                        {text: "Linux (Ubuntu/Debian)", link: "/guide/installation/linux"},
                        {text: "Cloud (EC2 / Terraform)", link: "/guide/installation/cloud"},
                    ],
                },
                {
                    text: "MongoDB Setup",
                    items: [
                        {text: "Overview", link: "/guide/mongodb/overview"},
                        {text: "Local (Atlas CLI)", link: "/guide/mongodb/local-atlas-cli"},
                        {text: "Atlas Cloud", link: "/guide/mongodb/atlas-cloud"},
                        {text: "Self-Hosted / Existing", link: "/guide/mongodb/self-hosted"},
                    ],
                },
                {
                    text: "Services",
                    items: [
                        {text: "STT Service", link: "/guide/services/stt"},
                        {text: "Hugging Face Authentication", link: "/guide/services/hugging-face-auth"},
                    ],
                },
                {
                    text: "First Run",
                    items: [
                        {text: "Generate Test Audio", link: "/guide/first-run/test-audio"},
                        {text: "Process an Earnings Call", link: "/guide/first-run/process"},
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