import { createApp } from 'vue'
import App from './App.vue'
import './style.css'
const initial = new URLSearchParams(location.search).get('theme')
document.documentElement.dataset.theme = initial === 'light' ? 'light' : 'dark'
window.addEventListener('message', e => {
  if (e.source !== window.parent) return
  if (e.data?.type === 'stimma-theme' && ['light','dark'].includes(e.data.theme)) document.documentElement.dataset.theme = e.data.theme
})
createApp(App).mount('#app')
