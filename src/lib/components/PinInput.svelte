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
  let popIndex = $state(-1);
  let maskedIndices = $state<Set<number>>(new Set());
  let maskTimer: ReturnType<typeof setTimeout> | null = null;
  let popTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimers() {
    if (maskTimer) { clearTimeout(maskTimer); maskTimer = null; }
    if (popTimer) { clearTimeout(popTimer); popTimer = null; }
  }

  function handleDigit(digit: string) {
    if (submitting || pinInput.length >= 4) return;
    const idx = pinInput.length;
    popIndex = idx;
    pinInput += digit;
    maskedIndices = new Set(maskedIndices);

    clearTimers();

    popTimer = setTimeout(() => { popIndex = -1; }, 220);
    maskTimer = setTimeout(() => {
      maskedIndices = new Set([...maskedIndices, idx]);
    }, 600);

    if (pinInput.length === 4) {
      onSubmit(pinInput);
    }
  }

  function handleBackspace() {
    if (submitting) return;
    clearTimers();
    popIndex = -1;
    pinInput = pinInput.slice(0, -1);
    const newMasked = new Set<number>();
    for (let i = 0; i < pinInput.length; i++) newMasked.add(i);
    maskedIndices = newMasked;
  }

  function handleClear() {
    if (submitting) return;
    clearTimers();
    popIndex = -1;
    pinInput = "";
    maskedIndices = new Set();
    onClear();
  }

  $effect(() => {
    if (errorMsg || success) {
      clearTimers();
      popIndex = -1;
      pinInput = "";
      maskedIndices = new Set();
    }
  });
</script>

<div class="pin-input-panel">
  <div class="panel-label">输入 4 位数字密码</div>
  <div class="pin-display">
    {#each Array(4) as _, i}
      <div class="pin-box" class:has-digit={i < pinInput.length} class:pop-in={popIndex === i && pinInput.length > i}>
        {#if i < pinInput.length && !maskedIndices.has(i)}
          {pinInput[i]}
        {:else if i < pinInput.length}
          <span class="pin-dot-inner"></span>
        {/if}
      </div>
    {/each}
  </div>

  {#if errorMsg}
    <div class="pin-error-banner"><span>{errorMsg}</span></div>
  {:else if submitting}
    <div class="pin-loading-banner">
      <span class="ld-dot"></span><span class="ld-dot"></span><span class="ld-dot"></span>
      <span>设置中</span>
    </div>
  {:else if success}
    <div class="pin-success-banner"><span>密码设置成功</span></div>
  {/if}

  <div class="numpad-wrap">
    <div class="numpad-overlay" class:active={submitting}>
      <div class="overlay-dots">
        <span class="od-dot"></span><span class="od-dot"></span><span class="od-dot"></span>
      </div>
    </div>
    <div class="numpad" class:dimmed={submitting}>
      {#each ["1","2","3","4","5","6","7","8","9"] as digit}
        <button class="num-key" onclick={() => handleDigit(digit)}>{digit}</button>
      {/each}
      <button class="num-key action-key" onclick={handleClear}>清除</button>
      <button class="num-key" onclick={() => handleDigit("0")}>0</button>
      <button class="num-key delete-key" onclick={handleBackspace}>⌫</button>
    </div>
  </div>
</div>

<style>
  .pin-input-panel {
    display: flex; flex-direction: column; align-items: stretch; gap: 8px;
    padding: 12px 16px 10px; background: rgba(15,23,42,.5);
    border: 1px solid rgba(255,255,255,.06); border-radius: 12px; margin-top: 4px;
    animation: slideDown .2s cubic-bezier(.16,1,.3,1);
  }
  @keyframes slideDown { from { opacity: 0; transform: translateY(-4px) scale(.98); } to { opacity: 1; transform: translateY(0) scale(1); } }
  .panel-label { font-size: 12px; color: rgba(255,255,255,.45); text-align: center; }
  .pin-display { display: flex; justify-content: center; gap: 8px; }
  .pin-box {
    width: 34px; height: 42px; border-radius: 8px; background: rgba(255,255,255,.05);
    border: 1.5px solid rgba(255,255,255,.12); display: flex; align-items: center;
    justify-content: center; font-size: 20px; font-weight: 700; color: transparent;
    transition: all .2s ease; position: relative; overflow: hidden;
  }
  .pin-box.has-digit { color: #60a5fa; border-color: rgba(96,165,250,.3); background: rgba(96,165,250,.06); }
  .pin-box.pop-in { animation: popInBox .2s cubic-bezier(.34,1.56,.64,1); }
  @keyframes popInBox { 0% { transform: scale(.6); opacity: 0; } 100% { transform: scale(1); opacity: 1; } }
  .pin-dot-inner { width: 8px; height: 8px; border-radius: 50%; background: #60a5fa; box-shadow: 0 0 6px rgba(96,165,250,.4); display: block; }

  .pin-error-banner {
    display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px;
    background: rgba(239,68,68,.1); border: 1px solid rgba(239,68,68,.2);
    border-radius: 8px; color: #fca5a5; font-size: 11px; font-weight: 500;
    animation: bannerIn .2s ease;
  }
  .pin-loading-banner {
    display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px;
    color: rgba(255,255,255,.5); font-size: 11px; font-weight: 500;
  }
  .pin-success-banner {
    display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px;
    background: rgba(34,197,94,.1); border: 1px solid rgba(34,197,94,.22);
    border-radius: 8px; color: #4ade80; font-size: 11px; font-weight: 600;
    animation: bannerIn .2s ease;
  }
  @keyframes bannerIn { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: translateY(0); } }
  .ld-dot, .od-dot {
    display: inline-block; width: 4px; height: 4px; border-radius: 50%;
    background: currentColor; animation: dotBounce 1.2s ease-in-out infinite;
  }
  .ld-dot:nth-child(2), .od-dot:nth-child(2) { animation-delay: .15s; }
  .ld-dot:nth-child(3), .od-dot:nth-child(3) { animation-delay: .3s; }
  @keyframes dotBounce { 0%,80%,100% { opacity: .3; transform: translateY(0); } 40% { opacity: 1; transform: translateY(-3px); } }

  .numpad-wrap { position: relative; }
  .numpad-overlay {
    position: absolute; inset: 0; background: rgba(15,23,42,.5);
    backdrop-filter: blur(2px); border-radius: 10px; display: flex;
    align-items: center; justify-content: center; z-index: 5;
    opacity: 0; pointer-events: none; transition: opacity .2s ease;
  }
  .numpad-overlay.active { opacity: 1; pointer-events: auto; }
  .overlay-dots { display: flex; gap: 5px; }
  .od-dot { background: rgba(255,255,255,.5); }
  .numpad {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px;
    width: 100%; transition: opacity .2s ease;
  }
  .numpad.dimmed { opacity: .4; }
  .num-key {
    height: 42px; border: 1px solid rgba(255,255,255,.1); border-radius: 8px;
    background: rgba(255,255,255,.05); color: #fff; font-size: 17px; font-weight: 500;
    cursor: pointer; transition: all .15s ease; user-select: none;
    position: relative; overflow: hidden;
  }
  .num-key:hover { background: rgba(255,255,255,.1); border-color: rgba(96,165,250,.25); }
  .num-key:active { background: rgba(96,165,250,.18); transform: scale(.96); }
  .num-key.action-key { font-size: 12px; color: rgba(255,255,255,.45); }
  .num-key.delete-key { font-size: 15px; color: rgba(255,255,255,.6); }
  .num-key.delete-key:hover { background: rgba(239,68,68,.12); border-color: rgba(239,68,68,.25); color: #fca5a5; }
</style>
