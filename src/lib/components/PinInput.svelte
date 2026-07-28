<!--
  PinInput 组件
  4 位数字密码输入面板，保持清晰的状态反馈，避免过度动效。
-->
<script lang="ts">
  interface Props {
    submitting: boolean;
    errorMsg: string;
    success: boolean;
    onSubmit: (pin: string) => void;
    onClear: () => void;
  }

  let { submitting, errorMsg, success, onSubmit, onClear }: Props = $props();

  let pinInput = $state("");
  let maskedIndices = $state<Set<number>>(new Set());
  let maskTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimers() {
    if (maskTimer) { clearTimeout(maskTimer); maskTimer = null; }
  }

  function handleDigit(digit: string) {
    if (submitting || pinInput.length >= 4) return;
    const idx = pinInput.length;
    pinInput += digit;
    maskedIndices = new Set(maskedIndices);

    clearTimers();
    maskTimer = setTimeout(() => {
      maskedIndices = new Set([...maskedIndices, idx]);
    }, 500);

    if (pinInput.length === 4) {
      onSubmit(pinInput);
    }
  }

  function handleBackspace() {
    if (submitting) return;
    clearTimers();
    pinInput = pinInput.slice(0, -1);
    const newMasked = new Set<number>();
    for (let i = 0; i < pinInput.length; i++) newMasked.add(i);
    maskedIndices = newMasked;
  }

  function handleClear() {
    if (submitting) return;
    clearTimers();
    pinInput = "";
    maskedIndices = new Set();
    onClear();
  }

  $effect(() => {
    if (errorMsg || success) {
      clearTimers();
      pinInput = "";
      maskedIndices = new Set();
    }
  });
</script>

<div class="pin-input-panel">
  <div class="pin-display">
    {#each Array(4) as _, i}
      <div class="pin-box" class:has-digit={i < pinInput.length} class:error={!!errorMsg}>
        {#if i < pinInput.length && !maskedIndices.has(i)}
          {pinInput[i]}
        {:else if i < pinInput.length}
          <span class="pin-dot"></span>
        {/if}
      </div>
    {/each}
  </div>

  {#if errorMsg}
    <div class="pin-banner error">{errorMsg}</div>
  {:else if success}
    <div class="pin-banner success">密码设置成功</div>
  {:else if submitting}
    <div class="pin-banner info">设置中…</div>
  {/if}

  <div class="numpad" class:dimmed={submitting}>
    {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as digit}
      <button class="num-key" onclick={() => handleDigit(digit)}>{digit}</button>
    {/each}
    <button class="num-key action" onclick={handleClear}>清除</button>
    <button class="num-key" onclick={() => handleDigit("0")}>0</button>
    <button class="num-key delete" onclick={handleBackspace}>⌫</button>
  </div>
</div>

<style>
  .pin-input-panel {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
    padding: 14px;
    background: #0f172a;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
  }

  .pin-display {
    display: flex;
    justify-content: center;
    gap: 10px;
  }

  .pin-box {
    width: 42px;
    height: 52px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.04);
    border: 1.5px solid rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    font-weight: 700;
    color: transparent;
    transition: border-color 0.15s ease, background-color 0.15s ease, color 0.15s ease;
  }

  .pin-box.has-digit {
    color: #60a5fa;
    border-color: rgba(59, 130, 246, 0.35);
    background: rgba(59, 130, 246, 0.08);
  }

  .pin-box.error {
    border-color: #ef4444;
    background: rgba(239, 68, 68, 0.1);
    animation: shake 0.35s ease;
  }

  .pin-box.error .pin-dot {
    background: #ef4444;
  }

  @keyframes shake {
    0%, 100% { transform: translateX(0); }
    25% { transform: translateX(-4px); }
    75% { transform: translateX(4px); }
  }

  .pin-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #60a5fa;
    display: block;
  }

  .pin-banner {
    text-align: center;
    padding: 6px 12px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 500;
  }

  .pin-banner.error {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.2);
    color: #fca5a5;
  }

  .pin-banner.success {
    background: rgba(16, 185, 129, 0.1);
    border: 1px solid rgba(16, 185, 129, 0.2);
    color: #34d399;
  }

  .pin-banner.info {
    color: rgba(255, 255, 255, 0.5);
  }

  .numpad {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    width: 100%;
    transition: opacity 0.15s ease;
  }

  .numpad.dimmed {
    opacity: 0.4;
    pointer-events: none;
  }

  .num-key {
    height: 46px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.9);
    font-size: 18px;
    font-weight: 500;
    cursor: pointer;
    transition: background-color 0.12s ease, border-color 0.12s ease;
    user-select: none;
  }

  .num-key:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(59, 130, 246, 0.25);
  }

  .num-key:active {
    background: rgba(59, 130, 246, 0.15);
  }

  .num-key.action {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.5);
  }

  .num-key.delete {
    font-size: 16px;
    color: rgba(255, 255, 255, 0.7);
  }

  .num-key.delete:hover {
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.2);
    color: #fca5a5;
  }
</style>
