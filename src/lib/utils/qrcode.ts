export async function generateQRCodeDataURL(text: string): Promise<string> {
  const qrcode = (await import("qrcode-generator")).default;
  const qr = qrcode(0, "M");
  qr.addData(text);
  qr.make();
  return qr.createDataURL(4, 0);
}
