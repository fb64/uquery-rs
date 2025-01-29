import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const config: Config = {
  title: 'µQuery',
  tagline: 'Enable easy and efficient access to your good old data files (CSV, Json, Parquet ...)',
  favicon: 'img/favicon.ico',

  // Set the production url of your site here
  url: 'https://uquery.flob.fr',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'facebook', // Usually your GitHub org/user name.
  projectName: 'docusaurus', // Usually your repo name.

  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
        },
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    // Replace with your project's social card
    image: 'img/docusaurus-social-card.jpg',
    navbar: {
      title: 'µQuery',
      logo: {
        alt: 'µQuery Logo',
        src: 'img/uquery.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'tutorialSidebar',
          position: 'left',
          label: 'Introduction',
        },
        {to: '/docs/quick-start', label: 'Quick Start', position: 'left'},
        {to: '/docs/category/advanced-tutorials', label: 'Advanced Tutorials', position: 'left'},
        {
          href: 'https://github.com/fb64/uquery-rs',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Introduction', to: '/docs/intro',
            },
            {
              label: 'Quick Start', to: '/docs/quick-start',
            },
            {
              label: 'Advanced Tutorial', to: '/docs/category/advanced-tutorials',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/fb64/uquery-rs',
            },
          ],
        },
      ],
      copyright: `µQuery Documentation ${new Date().getFullYear()}. Built with <a target="_blank" href="https://docusaurus.io/">Docusaurus</a>.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash'],
    },
    colorMode:{
      defaultMode: 'light',
      respectPrefersColorScheme: false,
    }
  } satisfies Preset.ThemeConfig,
};

export default config;
