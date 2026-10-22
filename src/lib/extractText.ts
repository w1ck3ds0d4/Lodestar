// Extract plain text from a resume file (PDF / DOCX / TXT / MD), in the webview.
// The heavy parsers are dynamically imported so they only load when a file is picked.

/**
 * Extract plain text from a resume file in the webview. Supports .txt, .md, .docx and .pdf;
 * any other extension throws. All results pass through {@link cleanImportedText}.
 */
export async function extractResumeText(file: File): Promise<string> {
  const name = file.name.toLowerCase();

  if (name.endsWith(".txt") || name.endsWith(".md")) {
    return cleanImportedText(await file.text());
  }

  if (name.endsWith(".docx")) {
    const mammoth = await import("mammoth");
    const arrayBuffer = await file.arrayBuffer();
    const { value } = await mammoth.extractRawText({ arrayBuffer });
    return cleanImportedText(value);
  }

  if (name.endsWith(".pdf")) {
    const pdfjs = await import("pdfjs-dist");
    // Point the worker at the bundled asset (same-origin, so it loads under the CSP).
    const workerUrl = (await import("pdfjs-dist/build/pdf.worker.min.mjs?url")).default;
    pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

    const data = new Uint8Array(await file.arrayBuffer());
    const loadingTask = pdfjs.getDocument({ data });
    const pdf = await loadingTask.promise;
    try {
      const pages: string[] = [];
      for (let i = 1; i <= pdf.numPages; i++) {
        const page = await pdf.getPage(i);
        const content = await page.getTextContent();
        pages.push(content.items.map((it) => ("str" in it ? it.str : "")).join(" "));
      }
      return cleanImportedText(pages.join("\n"));
    } finally {
      // Release the pdf.js document and its worker once extraction is done.
      void loadingTask.destroy();
    }
  }

  throw new Error("Unsupported file - use PDF, DOCX, TXT, or MD.");
}

// Strip icon-font junk that PDF/DOCX text extraction leaves behind. PDFs that render contact
// icons (email, phone, location) with an icon font extract those glyphs as stray characters -
// a lone accented letter, section sign, hash, or an emoji - which then pollute the resume
// context. Only WHITESPACE-BOUNDED single characters are touched, so real text is untouched:
// accented letters inside words and connectors like & | / - + survive.
export function cleanImportedText(s: string): string {
  return (
    s
      // Control characters are always extraction junk.
      .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F-\x9F]/g, " ")
      // A standalone single NON-ASCII letter is almost always a mis-mapped icon; ASCII single
      // letters ("I", "a") and accented letters inside words are kept.
      .replace(/(^|\s)(\p{L})(?=\s|$)/gu, (m, pre, ch) => (/[A-Za-z]/.test(ch) ? m : pre))
      // Standalone icon stand-in symbols and emoji, keeping real text connectors.
      .replace(/(^|\s)[#§¶©®™¤†‡](?=\s|$)/g, "$1")
      .replace(/(^|\s)\p{Extended_Pictographic}(?=\s|$)/gu, "$1")
      .replace(/[ \t]{2,}/g, " ")
      .replace(/[ \t]+\n/g, "\n")
      .replace(/\n{3,}/g, "\n\n")
      .trim()
  );
}
