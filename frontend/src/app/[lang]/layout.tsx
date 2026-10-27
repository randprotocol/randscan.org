import type { Metadata } from 'next';
import localFont from 'next/font/local';
import { notFound } from 'next/navigation';
import { Header } from '@/components/Header';
import { Footer } from '@/components/Footer';
import { ChainBanner } from '@/components/ChainStatus';
import { ChainMeta } from '@/components/ChainMeta';
import { I18nProvider } from '@/i18n/client';
import { getMessages } from '@/i18n/messages';
import { LOCALE_CODES, isLocale, localeInfo, localizePath } from '@/i18n/locales';
import '../globals.css';

// The wallet's three faces, self-hosted (src/fonts/SOURCES.txt): Inter for text, JetBrains Mono
// for hashes and keys, Departure Mono for the wordmark and the big figures. Scripts Inter lacks
// (CJK, Arabic) fall back to the system's own faces (globals.css, html[lang]).
const sans = localFont({
  src: '../../fonts/Inter-Variable.woff2',
  weight: '100 900',
  variable: '--font-sans',
  display: 'swap',
});

const mono = localFont({
  src: '../../fonts/JetBrainsMono-Variable.woff2',
  weight: '100 800',
  variable: '--font-mono',
  display: 'swap',
});

const display = localFont({
  src: '../../fonts/DepartureMono-Regular.woff2',
  weight: '400',
  variable: '--font-display',
  display: 'swap',
});

type Params = { params: Promise<{ lang: string }> };

/** Every page is built once per language; English is served unprefixed (src/middleware.ts). */
export function generateStaticParams() {
  return LOCALE_CODES.map((lang) => ({ lang }));
}

export const dynamicParams = false;

// metadataBase makes the link-preview card (app/opengraph-image.png, drawn by randprotocol.org's
// design/og/make.mjs) an absolute URL, which scrapers require.
export async function generateMetadata({ params }: Params): Promise<Metadata> {
  const { lang } = await params;
  const code = isLocale(lang) ? lang : 'en';
  const m = getMessages(code);
  return {
    metadataBase: new URL('https://randscan.org'),
    title: m.common.metaTitle,
    description: m.common.metaDescription,
    keywords: ['Rand Protocol', 'RAND', 'blockchain', 'explorer', 'blocks', 'transactions'],
    openGraph: {
      type: 'website',
      url: `https://randscan.org${localizePath(code, '/')}`,
      siteName: 'RandScan',
      title: m.common.metaTitle,
      description: m.common.metaDescription,
      locale: localeInfo(code).tag.replace(/-/g, '_'),
    },
    twitter: { card: 'summary_large_image' },
  };
}

/**
 * Applies the stored theme before first paint, or follows the operating system when nothing is
 * stored (the wallet does the same), so the wrong theme never flashes.
 */
const themeScript = `(function(){var d=document.documentElement;try{var t=localStorage.getItem('randscan-theme');if(t!=='dark'&&t!=='light'){t=window.matchMedia('(prefers-color-scheme: light)').matches?'light':'dark';}d.setAttribute('data-theme',t);}catch(e){d.setAttribute('data-theme','dark');}})();`;

export default async function RootLayout({
  children,
  params,
}: {
  children: React.ReactNode;
  params: Promise<{ lang: string }>;
}) {
  const { lang } = await params;
  if (!isLocale(lang)) notFound();
  const info = localeInfo(lang);
  const messages = getMessages(lang);
  return (
    <html lang={info.tag} dir={info.dir} suppressHydrationWarning>
      <head>
        <meta name="color-scheme" content="dark light" />
        <ChainMeta />
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body
        className={`${sans.variable} ${mono.variable} ${display.variable} min-h-screen bg-bg font-sans text-text`}
      >
        <I18nProvider locale={lang} messages={messages}>
          <div className="flex min-h-screen flex-col">
            <Header />
            <ChainBanner />
            <main className="flex-1">
              <div className="mx-auto max-w-6xl px-6 py-10 lg:px-8">{children}</div>
            </main>
            <Footer />
          </div>
        </I18nProvider>
      </body>
    </html>
  );
}
