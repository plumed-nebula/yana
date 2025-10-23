import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

/**
 * 批量选择 Store
 * 管理图库批量选择的全局状态，减少组件间通信
 */
export const useBatchSelectStore = defineStore('batchSelect', () => {
  // ========== 状态 ==========

  /** 是否进入批量选择模式 */
  const batchMode = ref(false);

  /** 已选中的 item.id 数组，按选择顺序保存 */
  const selectedOrder = ref<number[]>([]);

  /** Ctrl+拖拽多选状态 */
  const isCtrlDragging = ref(false);

  /** 拖拽起点卡片 ID */
  const dragStartId = ref<number | null>(null);

  /** 拖拽过程中扫过的卡片 ID Set */
  const draggedIds = ref<Set<number>>(new Set());

  /** 卡片 DOM 引用映射：id -> ref 元素 */
  const cardElements = ref<Map<number, HTMLElement>>(new Map());

  // ========== 计算属性 ==========

  /** 优化：使用 Set 提升 isSelected 查询性能从 O(n) 到 O(1) */
  const selectedSet = computed(() => new Set(selectedOrder.value));

  /** 已选中的卡片数 */
  const selectionCount = computed(() => selectedOrder.value.length);

  /** 是否有卡片被选中 */
  const hasSelection = computed(() => selectedOrder.value.length > 0);

  // ========== 方法 ==========

  /**
   * 切换批量模式
   */
  function toggleBatchMode() {
    batchMode.value = !batchMode.value;
    if (!batchMode.value) {
      // 退出批量模式时清空选择
      clearSelection();
    }
  }

  /**
   * 检查某个卡片是否被选中
   */
  function isSelected(id: number): boolean {
    return selectedSet.value.has(id);
  }

  /**
   * 获取卡片的选中序号（1-based）
   */
  function selectedIndex(id: number): number {
    const idx = selectedOrder.value.indexOf(id);
    return idx === -1 ? -1 : idx + 1;
  }

  /**
   * 切换单个卡片的选中状态
   */
  function toggleSelectItem(id: number) {
    const idx = selectedOrder.value.indexOf(id);
    if (idx === -1) {
      selectedOrder.value.push(id);
    } else {
      selectedOrder.value.splice(idx, 1);
    }
  }

  /**
   * 选中卡片
   */
  function selectItem(id: number) {
    if (!isSelected(id)) {
      selectedOrder.value.push(id);
    }
  }

  /**
   * 取消选中卡片
   */
  function deselectItem(id: number) {
    const idx = selectedOrder.value.indexOf(id);
    if (idx !== -1) {
      selectedOrder.value.splice(idx, 1);
    }
  }

  /**
   * 清空所有选择
   */
  function clearSelection() {
    selectedOrder.value = [];
  }

  /**
   * 注册卡片 DOM 元素
   */
  function registerCardElement(id: number, element: HTMLElement) {
    cardElements.value.set(id, element);
  }

  /**
   * 取消注册卡片 DOM 元素
   */
  function unregisterCardElement(id: number) {
    cardElements.value.delete(id);
  }

  /**
   * 获取指定卡片的 BoundingClientRect
   */
  function getCardBounds(id: number): DOMRect | null {
    const element = cardElements.value.get(id);
    return element ? element.getBoundingClientRect() : null;
  }

  /**
   * 检查两个矩形是否相交
   */
  function rectsIntersect(
    rect1: DOMRect | null,
    rect2: DOMRect | null
  ): boolean {
    if (!rect1 || !rect2) return false;
    return !(
      rect1.right < rect2.left ||
      rect1.left > rect2.right ||
      rect1.bottom < rect2.top ||
      rect1.top > rect2.bottom
    );
  }

  /**
   * 获取与拖拽矩形相交的所有卡片 ID
   * @param startX - 起点 X 坐标
   * @param startY - 起点 Y 坐标
   * @param endX - 终点 X 坐标
   * @param endY - 终点 Y 坐标
   */
  function getIntersectingCards(
    startX: number,
    startY: number,
    endX: number,
    endY: number
  ): number[] {
    const minX = Math.min(startX, endX);
    const maxX = Math.max(startX, endX);
    const minY = Math.min(startY, endY);
    const maxY = Math.max(startY, endY);

    const dragRect = new DOMRect(minX, minY, maxX - minX, maxY - minY);
    const result: number[] = [];

    for (const [id] of cardElements.value) {
      const cardBounds = getCardBounds(id);
      if (rectsIntersect(dragRect, cardBounds)) {
        result.push(id);
      }
    }

    return result;
  }

  /**
   * 开始 Ctrl 拖拽选择
   */
  function startCtrlDrag(itemId: number) {
    isCtrlDragging.value = true;
    dragStartId.value = itemId;
    draggedIds.value.clear();

    // 选中起点卡片
    if (!isSelected(itemId)) {
      selectItem(itemId);
    }
  }

  /**
   * 结束 Ctrl 拖拽选择
   */
  function endCtrlDrag() {
    isCtrlDragging.value = false;
    dragStartId.value = null;
    draggedIds.value.clear();
  }

  /**
   * 批量选择卡片（用于拖拽过程）
   */
  function selectMultiple(ids: number[]) {
    for (const id of ids) {
      if (!draggedIds.value.has(id)) {
        draggedIds.value.add(id);
        if (!isSelected(id)) {
          selectItem(id);
        }
      }
    }
  }

  /**
   * 获取已选中的卡片 ID 列表
   */
  function getSelectedIds(): number[] {
    return selectedOrder.value.slice();
  }

  return {
    // 状态
    batchMode,
    selectedOrder,
    isCtrlDragging,
    dragStartId,
    draggedIds,
    cardElements,

    // 计算属性
    selectedSet,
    selectionCount,
    hasSelection,

    // 方法
    toggleBatchMode,
    isSelected,
    selectedIndex,
    toggleSelectItem,
    selectItem,
    deselectItem,
    clearSelection,
    registerCardElement,
    unregisterCardElement,
    getCardBounds,
    rectsIntersect,
    getIntersectingCards,
    startCtrlDrag,
    endCtrlDrag,
    selectMultiple,
    getSelectedIds,
  };
});
