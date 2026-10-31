const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1600, height: 800 } });
  await p.goto('file:///tmp/banner/banner.html');
  await p.waitForTimeout(400);
  await p.screenshot({ path: '/tmp/banner/banner.png' });
  await b.close();
})();
