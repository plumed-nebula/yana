/**
 * 自定义指令：v-register-card
 * 用于自动将卡片 DOM 元素注册到 Pinia store
 */
import type { DirectiveBinding } from 'vue';
import { useBatchSelectStore } from '../stores/batchSelect';

export const vRegisterCard = {
  mounted(el: HTMLElement, binding: DirectiveBinding<number>) {
    const itemId = binding.value;
    const store = useBatchSelectStore();
    store.registerCardElement(itemId, el);
  },
  unmounted(_el: HTMLElement, binding: DirectiveBinding<number>) {
    const itemId = binding.value;
    const store = useBatchSelectStore();
    store.unregisterCardElement(itemId);
  },
};
