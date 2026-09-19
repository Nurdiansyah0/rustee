import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import './index.css'
import { useSubscriptionStore } from './stores/subscription'
import { useAuthStore } from './stores/auth'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)
app.mount('#app')

if (typeof window !== 'undefined') {
  const subStore = useSubscriptionStore(pinia)
  window.subscriptionStore = subStore
  window.subcribtionStore = subStore
  window.authStore = useAuthStore(pinia)
}
