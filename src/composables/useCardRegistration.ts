/**
 * 卡片自动注册到 Pinia store 的组合式函数
 * 当卡片 DOM 挂载时，将其元素引用注册到 batchSelectStore
 */
import { useBatchSelectStore } from '../stores/batchSelect';
import { onMounted, onBeforeUnmount } from 'vue';

export function useCardRegistration(
  itemId: number,
  cardElement: HTMLElement | null
) {
  const store = useBatchSelectStore();

  onMounted(() => {
    if (cardElement) {
      store.registerCardElement(itemId, cardElement);
    }
  });

  onBeforeUnmount(() => {
    store.unregisterCardElement(itemId);
  });
}
