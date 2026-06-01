// Extract plain text from a resume file (PDF / DOCX / TXT / MD), in the webview.
// The heavy parsers are dynamically imported so they only load when a file is picked.

export async function extractResumeText(file: File): Promise<string> {
  const name = file.name.toLowerCase();

  if (name.endsWith(".txt") || name.endsWith(".md")) {
    return (await file.text()).trim();
  }

  if (name.endsWith(".docx")) {
    const mammoth = await import("mammoth");
    const arrayBuffer = await file.arrayBuffer();
    const { value } = await mammoth.extractRawText({ arrayBuffer });
    return value.trim();
  }

  if (name.endsWith(".pdf")) {
    const pdfjs = await import("pdfjs-dist");
    // Point the worker at the bundled asset (same-origin, so it loads under the CSP).
    const workerUrl = (await import("pdfjs-dist/build/pdf.worker.min.mjs?url")).default;
    pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

    const data = new Uint8Array(await file.arrayBuffer());
    const pdf = await pdfjs.getDocument({ data }).promise;
    const pages: string[] = [];
    for (let i = 1; i <= pdf.numPages; i++) {
      const page = await pdf.getPage(i);
      const content = await page.getTextContent();
      pages.push(content.items.map((it) => ("str" in it ? it.str : "")).join(" "));
    }
    return pages.join("\n").replace(/[ \t]+\n/g, "\n").trim();
  }

  throw new Error("Unsupported file - use PDF, DOCX, TXT, or MD.");
}
