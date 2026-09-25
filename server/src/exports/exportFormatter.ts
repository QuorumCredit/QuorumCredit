import type { CredentialData, ExportFormat } from "./exportStore.js";

/**
 * Issue #1584: Export formatter for converting credential data to various formats
 * (JSON, PDF, CSV) for credential holder export functionality.
 */

export class ExportFormatter {
  /**
   * Format credential data to the specified format.
   */
  format(
    credentialId: string,
    data: CredentialData,
    format: ExportFormat
  ): Buffer {
    switch (format) {
      case "json":
        return this.formatJson(data);
      case "csv":
        return this.formatCsv(credentialId, data);
      case "pdf":
        return this.formatPdf(credentialId, data);
      default:
        throw new Error(`Unsupported export format: ${format}`);
    }
  }

  /**
   * Format as JSON.
   */
  private formatJson(data: CredentialData): Buffer {
    const json = JSON.stringify(data, null, 2);
    return Buffer.from(json, "utf-8");
  }

  /**
   * Format as CSV.
   */
  private formatCsv(credentialId: string, data: CredentialData): Buffer {
    const headers = [
      "Credential ID",
      "Holder",
      "Issue Date",
      "Expiry Date",
      "Status",
      "Metadata",
    ];

    const rows = [
      headers.join(","),
      [
        credentialId,
        data.holder,
        new Date(data.issueDate).toISOString(),
        new Date(data.expiryDate).toISOString(),
        data.status,
        JSON.stringify(data.metadata || {}),
      ]
        .map((v) => `"${String(v).replace(/"/g, '""')}"`)
        .join(","),
    ];

    return Buffer.from(rows.join("\n"), "utf-8");
  }

  /**
   * Format as PDF (simplified implementation using text-based PDF).
   */
  private formatPdf(credentialId: string, data: CredentialData): Buffer {
    // This is a simplified implementation that generates a basic PDF structure
    // In production, you would use a library like pdfkit or puppeteer
    const pdfContent = this.generatePdfContent(credentialId, data);
    return Buffer.from(pdfContent, "utf-8");
  }

  /**
   * Generate basic PDF content (text-based).
   */
  private generatePdfContent(credentialId: string, data: CredentialData): string {
    const issueDate = new Date(data.issueDate).toISOString().split("T")[0];
    const expiryDate = new Date(data.expiryDate).toISOString().split("T")[0];

    return `%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /Resources 4 0 R /MediaBox [0 0 612 792] /Contents 5 0 R >>
endobj
4 0 obj
<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>
endobj
5 0 obj
<< /Length 500 >>
stream
BT
/F1 12 Tf
50 750 Td
(Credential Export) Tj
0 -20 Td
(Credential ID: ${credentialId}) Tj
0 -20 Td
(Holder: ${data.holder}) Tj
0 -20 Td
(Issue Date: ${issueDate}) Tj
0 -20 Td
(Expiry Date: ${expiryDate}) Tj
0 -20 Td
(Status: ${data.status}) Tj
ET
endstream
endobj
xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000207 00000 n
0000000301 00000 n
trailer
<< /Size 6 /Root 1 0 R >>
startxref
850
%%EOF`;
  }

  /**
   * Get the MIME type for an export format.
   */
  getMimeType(format: ExportFormat): string {
    switch (format) {
      case "json":
        return "application/json";
      case "csv":
        return "text/csv";
      case "pdf":
        return "application/pdf";
      default:
        return "application/octet-stream";
    }
  }

  /**
   * Get the file extension for an export format.
   */
  getFileExtension(format: ExportFormat): string {
    return format;
  }

  /**
   * Generate a filename for the export.
   */
  generateFilename(credentialId: string, format: ExportFormat): string {
    const timestamp = new Date().toISOString().split("T")[0];
    return `credential_${credentialId}_${timestamp}.${this.getFileExtension(format)}`;
  }
}
