// Runs before first paint so the saved skin and light/dark mode apply without a flash.
try {
  var t = localStorage.getItem('theme'), s = localStorage.getItem('skin');
  if (t === 'dark' || t === 'light') document.documentElement.setAttribute('data-theme', t);
  if (s && s !== 'aurora') document.documentElement.setAttribute('data-skin', s);
} catch (e) {}
