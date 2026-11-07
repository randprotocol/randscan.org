import { Fragment } from 'react';
import { PageHeader } from './States';
import { Rich } from '@/i18n/RichText';
import { fill } from '@/i18n/rich';

/** A section of a legal document: a heading and its paragraphs or lists. */
export interface LegalSection {
  id: string;
  title: string;
  body: React.ReactNode;
}

/**
 * A section as the dictionary holds it: each body string is a paragraph, or, when it starts with
 * "- ", an item of a bulleted list (consecutive items form one list).
 */
export interface LegalSectionText {
  id: string;
  title: string;
  body: string[];
}

/** The document's fixed words, in the page's language (namespace `legal`). */
export interface LegalWords {
  /** "Legal" */
  label: string;
  /** "Effective {date}", already filled. */
  effective: string;
  /** "Contents" */
  contents: string;
  /** "See also the <a href=\"{href}\">{label}</a>." — filled here from `related`. */
  seeAlso: string;
  /** Shown under the header on a translated page: the English text governs. */
  notice?: string;
}

interface LegalPageProps {
  title: string;
  /** The date the text took effect, already formatted ("1 October 2026"); shown through `words.effective`. */
  effective: string;
  /** One or two sentences under the title saying what the document covers. */
  intro: React.ReactNode;
  sections: LegalSection[];
  /** The other legal document, linked at the foot of this one. */
  related: { href: string; label: string };
  /** The page's language code, for links inside the dictionary's text. */
  lang: string;
  words: LegalWords;
}

/** Dictionary sections as rendered sections: paragraphs and lists, with markup and placeholders. */
export function legalSections(
  sections: LegalSectionText[],
  lang: string,
  vars?: Record<string, string | number>,
): LegalSection[] {
  return sections.map((section) => {
    const blocks: React.ReactNode[] = [];
    let items: string[] = [];
    const flush = () => {
      if (items.length === 0) return;
      blocks.push(
        <ul key={blocks.length} className="list-disc space-y-1 ps-5">
          {items.map((item, i) => (
            <li key={i}>
              <Rich text={fill(item, vars)} locale={lang} />
            </li>
          ))}
        </ul>,
      );
      items = [];
    };
    for (const text of section.body) {
      if (text.startsWith('- ')) {
        items.push(text.slice(2));
        continue;
      }
      flush();
      blocks.push(
        <p key={blocks.length}>
          <Rich text={fill(text, vars)} locale={lang} />
        </p>,
      );
    }
    flush();
    return {
      id: section.id,
      title: section.title,
      body: (
        <>
          {blocks.map((block, i) => (
            <Fragment key={i}>{block}</Fragment>
          ))}
        </>
      ),
    };
  });
}

/**
 * The layout shared by /privacy and /terms: the page header, a contents list, numbered sections
 * with anchors, and a pointer to the companion document. Plain text, in the body face, at a
 * reading width.
 */
export function LegalPage({ title, intro, sections, related, lang, words }: LegalPageProps) {
  return (
    <div className="mx-auto max-w-3xl">
      <PageHeader title={title} label={words.label} subtitle={<>{words.effective}</>} />

      {words.notice && <p className="-mt-4 mb-8 text-sm text-mute">{words.notice}</p>}

      <div className="legal">
        <p className="text-base text-text">{intro}</p>

        <nav aria-label={words.contents} className="card-padded mt-8">
          <h2 className="chip">{words.contents}</h2>
          <ol className="mt-3 columns-1 gap-x-8 space-y-1.5 text-sm sm:columns-2">
            {sections.map((section, index) => (
              <li key={section.id}>
                <a href={`#${section.id}`} className="link">
                  <span className="me-2 font-mono text-xs text-mute">{index + 1}</span>
                  {section.title}
                </a>
              </li>
            ))}
          </ol>
        </nav>

        {sections.map((section, index) => (
          <section key={section.id} id={section.id} className="mt-10 scroll-mt-6">
            <h2 className="text-xl font-medium text-strong">
              <span className="me-3 font-mono text-sm text-mute">{index + 1}</span>
              {section.title}
            </h2>
            <div className="mt-3 space-y-3 text-[0.9375rem] leading-relaxed text-soft [&_code]:font-mono [&_code]:text-xs">
              {section.body}
            </div>
          </section>
        ))}

        <p className="mt-12 border-t border-border-soft pt-6 text-sm text-mute">
          <Rich text={fill(words.seeAlso, { href: related.href, label: related.label })} locale={lang} />
        </p>
      </div>
    </div>
  );
}
