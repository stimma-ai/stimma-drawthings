const color = name => `rgb(var(--${name}) / <alpha-value>)`
export default {
  content: ['./index.html', './src/**/*.{vue,js}'],
  theme: { extend: {
    colors: Object.fromEntries(['base','surface','surface-raised','surface-hover','content','content-secondary','content-tertiary','edge','accent','selection','success','failure','running'].map(n => [n,color(n)])),
    fontFamily: { brand: ['General Sans','system-ui','sans-serif'] },
    borderRadius: { md:'6px',lg:'8px' },
  } },
}
