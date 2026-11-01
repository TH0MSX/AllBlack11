// Renders banner.html to ../banner.png (1600x800). Run from anywhere: node docs/banner-source/shot.js
const { chromium } = require('playwright');
const path = require('path');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 800 } });
  await page.goto('file://' + path.join(__dirname, 'banner.html'));
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(__dirname, '..', 'banner.png') });
  await browser.close();
})();
