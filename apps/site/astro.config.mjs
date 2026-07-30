// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import sitemap from '@astrojs/sitemap';
import rehypeLegal from './src/lib/rehype-legal.mjs';

// Домен и base вынесены в env, чтобы переезд с github.io на собственный домен
// был одной правкой окружения, а не переписыванием ссылок по всему контенту.
// Дефолт — то, где сайт живёт сейчас: https://zdllucky.github.io/wotold/
const SITE = process.env.SITE_URL ?? 'https://zdllucky.github.io';
const BASE = process.env.SITE_BASE ?? '/wotold';

export default defineConfig({
  site: SITE,
  base: BASE,
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'Wotold',
      description:
        'Запись звонков с транскрипцией, диаризацией и саммари — полностью на твоём Mac, без сети.',
      // Собственный лендинг живёт в src/pages/index.astro и перекрывает
      // корневой роут Starlight. Остальные страницы — контент-коллекция.
      defaultLocale: 'root',
      locales: {
        root: { label: 'Русский', lang: 'ru' },
        en: { label: 'English', lang: 'en' },
        kk: { label: 'Қазақша', lang: 'kk' },
      },
      social: [
        {
          icon: 'github',
          label: 'GitHub',
          href: 'https://github.com/zdllucky/wotold',
        },
      ],
      editLink: {
        baseUrl: 'https://github.com/zdllucky/wotold/edit/main/apps/site/',
      },
      customCss: ['./src/styles/site.css', './src/styles/legal.css'],
      // Правовой шаблон (SPEC §7): kicker над h1 + хаб документов.
      // Оба override пропускают не-legal роуты в дефолт без изменений.
      components: {
        PageTitle: './src/components/starlight/PageTitle.astro',
        MarkdownContent: './src/components/starlight/MarkdownContent.astro',
      },
      // Ноль сторонних хостов — то же обещание, что даёт само приложение.
      // Шрифты синхронизируются из десктопа скриптом sync-fonts.mjs.
      head: [
        {
          // Статичная OG-карточка из марки (SPEC §8). Генерируется разово
          // scripts/gen-og.mjs, артефакт закоммичен в public/.
          tag: 'meta',
          attrs: { property: 'og:image', content: `${SITE}${BASE}/og.png` },
        },
        {
          tag: 'meta',
          attrs: { name: 'twitter:card', content: 'summary_large_image' },
        },
        {
          tag: 'link',
          attrs: {
            rel: 'preload',
            href: `${BASE}/fonts/onest-cyrillic-400-800.woff2`,
            as: 'font',
            type: 'font/woff2',
            crossorigin: 'anonymous',
          },
        },
        {
          // @font-face живут в public/, а не в src/: их пути относительны
          // самого файла, поэтому base здесь роли не играет. Генерируется
          // scripts/sync-fonts.mjs из apps/desktop/src/styles/fonts.css.
          tag: 'link',
          attrs: { rel: 'stylesheet', href: `${BASE}/fonts/fonts.css` },
        },
      ],
      sidebar: [
        {
          label: 'Продукт',
          translations: { en: 'Product', kk: 'Өнім' },
          items: [
            { slug: 'features' },
            { slug: 'download' },
            { slug: 'how-it-works' },
            { slug: 'mcp' },
          ],
        },
        // Правовой раздел намеренно НЕ в сайдбаре. Политика, уведомление о
        // записи, условия и лицензия — не документация продукта: у них другой
        // жанр, другой регистр и другой читатель. Живут под /legal/ с
        // собственной страницей-оглавлением, скрыты из навигации доков
        // (sidebar.hidden во фронтматтере) и вынесены в подвал лендинга.
        {
          label: 'Проект',
          translations: { en: 'Project', kk: 'Жоба' },
          items: [
            { slug: 'about' },
            { slug: 'roadmap' },
            { slug: 'faq' },
            { slug: 'support' },
            { slug: 'contributing' },
          ],
        },
      ],
    }),
    sitemap(),
  ],
  markdown: {
    // Mono-нумерация правовых пунктов на билд-слое — контент не трогается.
    rehypePlugins: [rehypeLegal],
  },
});
