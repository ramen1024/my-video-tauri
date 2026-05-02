<script lang="ts">
  import { getPasswordStatus, setPasswordEnabled, setPassword, generateRandomPassword, resetPassword } from "$lib/services/password";
  import type { PasswordStatus } from "$lib/types";

  interface Props {
    passwordStatus: PasswordStatus;
    onStatusChange: (status: PasswordStatus) => void;
    onError: (msg: string) => void;
  }

  let { passwordStatus, onStatusChange, onError }: Props = $props();

  let pwdPanelExpanded = $state(false);
  let pwdInput = $state("");
  let pwdErrorMsg = $state("");
  let pwdSubmitting = $state(false);
  let pwdCopied = $state(false);
  let popIndex = $state(-1);
  let maskedIndices = $state<Set<number>>(new Set());
  let pwdSuccess = $state(false);

  let passwordEnabled = $derived(passwordStatus.enabled);
  let hasPassword = $derived(passwordStatus.has_password);
  let currentPassword = $derived(passwordStatus.password);

  async function toggleProtection() {
    if (!hasPassword && !passwordEnabled) {
      pwdPanelExpanded = true;
      return;
    }
    try {
      await setPasswordEnabled(!passwordEnabled);
      onStatusChange({ ...passwordStatus, enabled: !passwordEnabled });
    } catch (e) {
      onError("切换密码保护失败: " + e);
    }
  }

  async function copyPassword() {
    try {
      await navigator.clipboard.writeText(currentPassword || "");
      pwdCopied = true;
      setTimeout(() => { pwdCopied = false; }, 1500);
    } catch (e) {
      onError("复制失败: " + e);
    }
  }

  async function randomGenerateAndApply() {
    try {
      const newPwd = await generateRandomPassword();
      await setPassword(newPwd);
      if (!passwordEnabled) {
        await setPasswordEnabled(true);
      }
      const status = await getPasswordStatus();
      onStatusChange(status);
      pwdInput = "";
      pwdErrorMsg = "";
    } catch (e) {
      pwdErrorMsg = "设置密码失败: " + e;
    }
  }

  function handleNumpadDigit(digit: string) {
    if (pwdSubmitting || pwdInput.length >= 4) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    const idx = pwdInput.length;
    popIndex = idx;
    pwdInput += digit;
    const newMasked = new Set(maskedIndices);
    maskedIndices = newMasked;
    setTimeout(() => {
      maskedIndices = new Set([...maskedIndices, idx]);
      popIndex = -1;
    }, 600);
    setTimeout(() => { popIndex = -1; }, 220);
    if (pwdInput.length === 4) {
      submitNewPassword();
    }
  }

  function handleNumpadBackspace() {
    if (pwdSubmitting) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    pwdInput = pwdInput.slice(0, -1);
    const newMasked = new Set<number>();
    for (let i = 0; i < pwdInput.length; i++) newMasked.add(i);
    maskedIndices = newMasked;
  }

  function handleNumpadClear() {
    if (pwdSubmitting) return;
    pwdErrorMsg = "";
    pwdSuccess = false;
    pwdInput = "";
    maskedIndices = new Set();
  }

  async function submitNewPassword() {
    if (pwdInput.length !== 4) return;
    pwdSubmitting = true;
    try {
      await setPassword(pwdInput);
      if (!passwordEnabled) {
        await setPasswordEnabled(true);
      }
      const status = await getPasswordStatus();
      onStatusChange(status);
      pwdSuccess = true;
      setTimeout(() => {
        pwdInput = "";
        pwdErrorMsg = "";
        pwdSuccess = false;
        maskedIndices = new Set();
        pwdPanelExpanded = false;
      }, 1200);
    } catch (e) {
      pwdErrorMsg = "设置失败: " + e;
      pwdInput = "";
      maskedIndices = new Set();
    } finally {
      pwdSubmitting = false;
    }
  }

  async function clearPassword() {
    try {
      await resetPassword();
      onStatusChange({ enabled: false, has_password: false, password: null });
      pwdInput = "";
      pwdPanelExpanded = false;
      maskedIndices = new Set();
      popIndex = -1;
      pwdSuccess = false;
    } catch (e) {
      onError("清除密码失败: " + e);
    }
  }

  function togglePwdPanel() {
    pwdPanelExpanded = !pwdPanelExpanded;
    pwdInput = "";
    pwdErrorMsg = "";
    pwdSuccess = false;
    maskedIndices = new Set();
    popIndex = -1;
  }
</script>

<div class="protection-toggle">
  <div class="protection-status" class:on={passwordEnabled} class:off={!passwordEnabled}>
    <span class="status-dot"></span>
    <span>{passwordEnabled ? "已开启" : "未开启"}</span>
  </div>
  <button class="toggle-btn" class:active={passwordEnabled} onclick={toggleProtection}>
    {passwordEnabled ? "关闭" : "开启"}
  </button>
</div>

<div class="pwd-section">
  {#if hasPassword && currentPassword}
    <div class="pwd-show-area">
      <div class="pwd-display-row">
        <span class="pwd-hint-text">{passwordEnabled ? "访问密码" : "已保存密码"}:</span>
        <div class="pwd-digits">
          {#each currentPassword.split('') as digit}
            <span class="pwd-digit">{digit}</span>
          {/each}
        </div>
      </div>
      <div class="pwd-actions-row">
        <button class="action-btn random-btn" onclick={randomGenerateAndApply}>换一个</button>
        <button class="action-btn copy-btn" onclick={copyPassword}>
          {pwdCopied ? "已复制" : "复制"}
        </button>
        <button class="action-btn clear-btn" onclick={clearPassword}>清除</button>
      </div>
    </div>
  {:else if !hasPassword}
    <div class="pwd-setup-area">
      <div class="setup-hint">设置4位数字密码，局域网访问需输入此密码</div>
      <button class="setup-random-btn" onclick={randomGenerateAndApply}>随机生成并启动保护</button>
      <button class="setup-manual-btn" onclick={togglePwdPanel}>手动输入密码</button>
    </div>
  {/if}

  {#if pwdPanelExpanded}
    <div class="pwd-input-panel">
      <div class="panel-label">输入 4 位数字密码</div>
      <div class="pin-display">
        {#each Array(4) as _, i}
          <div class="pin-box" class:has-digit={i < pwdInput.length} class:pop-in={popIndex === i && pwdInput.length > i}>
            {#if i < pwdInput.length && !maskedIndices.has(i)}
              {pwdInput[i]}
            {:else if i < pwdInput.length}
              <span class="pin-dot-inner"></span>
            {/if}
          </div>
        {/each}
      </div>

      {#if pwdErrorMsg}
        <div class="pwd-error-banner"><span>{pwdErrorMsg}</span></div>
      {:else if pwdSubmitting}
        <div class="pwd-loading-banner">
          <span class="ld-dot"></span><span class="ld-dot"></span><span class="ld-dot"></span>
          <span>设置中</span>
        </div>
      {:else if pwdSuccess}
        <div class="pwd-success-banner"><span>密码设置成功</span></div>
      {/if}

      <div class="numpad-wrap">
        <div class="numpad-overlay" class:active={pwdSubmitting}>
          <div class="overlay-dots">
            <span class="od-dot"></span><span class="od-dot"></span><span class="od-dot"></span>
          </div>
        </div>
        <div class="numpad" class:dimmed={pwdSubmitting}>
          {#each ["1","2","3","4","5","6","7","8","9"] as digit}
            <button class="num-key" onclick={() => handleNumpadDigit(digit)}>{digit}</button>
          {/each}
          <button class="num-key action-key" onclick={handleNumpadClear}>清除</button>
          <button class="num-key" onclick={() => handleNumpadDigit("0")}>0</button>
          <button class="num-key delete-key" onclick={handleNumpadBackspace}>⌫</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .protection-toggle { display: flex; align-items: center; gap: 8px; }
  .protection-status {
    display: flex; align-items: center; gap: 6px; padding: 4px 10px;
    border-radius: 20px; font-size: 12px; font-weight: 500;
  }
  .protection-status.on { background: rgba(34,197,94,.12); border: 1px solid rgba(34,197,94,.25); color: #4ade80; }
  .protection-status.off { background: rgba(255,255,255,.04); border: 1px solid rgba(255,255,255,.08); color: rgba(255,255,255,.4); }
  .status-dot { width: 6px; height: 6px; border-radius: 50%; display: inline-block; }
  .protection-status.on .status-dot { background: #4ade80; box-shadow: 0 0 6px rgba(74,222,128,.5); }
  .protection-status.off .status-dot { background: rgba(255,255,255,.25); }
  .toggle-btn {
    padding: 4px 12px; font-size: 12px; font-weight: 500; border-radius: 8px;
    cursor: pointer; transition: all .2s ease; border: 1px solid rgba(255,255,255,.12);
    background: rgba(255,255,255,.06); color: rgba(255,255,255,.7);
  }
  .toggle-btn.active { background: rgba(34,197,94,.15); border-color: rgba(34,197,94,.3); color: #4ade80; }
  .toggle-btn:hover { background: rgba(255,255,255,.1); }
  .toggle-btn.active:hover { background: rgba(34,197,94,.25); }

  .pwd-section {
    display: flex; flex-direction: column; gap: 8px; padding: 12px;
    background: rgba(59,130,246,.04); border: 1px solid rgba(59,130,246,.1);
    border-radius: 12px; margin-top: 4px;
  }
  .pwd-show-area { display: flex; flex-direction: column; gap: 8px; }
  .pwd-display-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .pwd-hint-text { font-size: 12px; color: rgba(255,255,255,.5); white-space: nowrap; }
  .pwd-digits { display: flex; gap: 6px; }
  .pwd-digit {
    font-size: 22px; font-weight: 700; color: #60a5fa; width: 32px; height: 38px;
    display: flex; align-items: center; justify-content: center;
    background: rgba(96,165,250,.1); border: 1px solid rgba(96,165,250,.2); border-radius: 8px;
  }
  .pwd-actions-row { display: flex; gap: 6px; flex-wrap: wrap; }
  .action-btn {
    padding: 5px 12px; font-size: 12px; font-weight: 500; cursor: pointer;
    border-radius: 8px; border: 1px solid rgba(255,255,255,.1);
    background: rgba(255,255,255,.05); color: rgba(255,255,255,.65); transition: all .2s ease;
  }
  .action-btn:hover { background: rgba(255,255,255,.1); }
  .random-btn:hover { border-color: rgba(34,197,94,.3); color: #4ade80; background: rgba(34,197,94,.08); }
  .copy-btn:hover { border-color: rgba(96,165,250,.3); color: #60a5fa; background: rgba(96,165,250,.08); }
  .clear-btn:hover { border-color: rgba(239,68,68,.3); color: #fca5a5; background: rgba(239,68,68,.08); }

  .pwd-setup-area { display: flex; flex-direction: column; gap: 8px; align-items: center; }
  .setup-hint { font-size: 12px; color: rgba(255,255,255,.45); text-align: center; }
  .setup-random-btn {
    padding: 8px 16px; font-size: 12px; font-weight: 600; border-radius: 10px;
    cursor: pointer; border: 1px solid rgba(34,197,94,.25); background: rgba(34,197,94,.12);
    color: #4ade80; transition: all .2s ease;
  }
  .setup-random-btn:hover { background: rgba(34,197,94,.22); }
  .setup-manual-btn {
    padding: 6px 14px; font-size: 12px; font-weight: 500; border-radius: 8px;
    cursor: pointer; border: 1px solid rgba(255,255,255,.1); background: rgba(255,255,255,.04);
    color: rgba(255,255,255,.55); transition: all .2s ease;
  }
  .setup-manual-btn:hover { background: rgba(255,255,255,.08); color: rgba(255,255,255,.8); }

  .pwd-input-panel {
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

  .pwd-error-banner {
    display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px;
    background: rgba(239,68,68,.1); border: 1px solid rgba(239,68,68,.2);
    border-radius: 8px; color: #fca5a5; font-size: 11px; font-weight: 500;
    animation: bannerIn .2s ease;
  }
  .pwd-loading-banner {
    display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px;
    color: rgba(255,255,255,.5); font-size: 11px; font-weight: 500;
  }
  .pwd-success-banner {
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
