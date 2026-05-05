<!--
  PasswordPanel 组件
  密码保护管理面板，支持开启/关闭保护、随机生成密码、手动输入密码、复制和清除
-->
<script lang="ts">
  import { getPasswordStatus, setPasswordEnabled, setPassword, generateRandomPassword, resetPassword } from "$lib/services/password";
  import type { PasswordStatus } from "$lib/types";
  import { parseAppError } from "$lib/types";
  import PinInput from "$lib/components/PinInput.svelte";

  interface Props {
    passwordStatus: PasswordStatus;
    onStatusChange: (status: PasswordStatus) => void;
    onError: (msg: string) => void;
  }

  let { passwordStatus, onStatusChange, onError }: Props = $props();

  let knownPassword = $state("");
  let pwdPanelExpanded = $state(false);
  let pwdErrorMsg = $state("");
  let pwdSubmitting = $state(false);
  let pwdCopied = $state(false);
  let pwdSuccess = $state(false);

  let passwordEnabled = $derived(passwordStatus.enabled);
  let hasPassword = $derived(passwordStatus.has_password);

  async function toggleProtection() {
    if (!hasPassword && !passwordEnabled) {
      pwdPanelExpanded = true;
      return;
    }
    try {
      await setPasswordEnabled(!passwordEnabled);
      onStatusChange({ ...passwordStatus, enabled: !passwordEnabled });
    } catch (e) {
      onError("切换密码保护失败: " + parseAppError(e));
    }
  }

  async function copyPassword() {
    if (!navigator.clipboard) {
      onError("当前环境不支持自动复制，请手动复制密码");
      return;
    }
    try {
      await navigator.clipboard.writeText(knownPassword);
      pwdCopied = true;
      setTimeout(() => { pwdCopied = false; }, 1500);
    } catch (e) {
      onError("复制失败: " + parseAppError(e));
    }
  }

  async function randomGenerateAndApply() {
    try {
      const newPwd = await generateRandomPassword();
      await setPassword(newPwd);
      if (!passwordEnabled) {
        await setPasswordEnabled(true);
      }
      knownPassword = newPwd;
      const status = await getPasswordStatus();
      onStatusChange(status);
      pwdErrorMsg = "";
      pwdPanelExpanded = false;
    } catch (e) {
      pwdErrorMsg = "设置密码失败: " + parseAppError(e);
    }
  }

  async function handlePinSubmit(pin: string) {
    if (pin.length !== 4) return;
    pwdSubmitting = true;
    try {
      await setPassword(pin);
      if (!passwordEnabled) {
        await setPasswordEnabled(true);
      }
      knownPassword = pin;
      const status = await getPasswordStatus();
      onStatusChange(status);
      pwdSuccess = true;
      setTimeout(() => {
        pwdErrorMsg = "";
        pwdSuccess = false;
        pwdPanelExpanded = false;
      }, 1200);
    } catch (e) {
      pwdErrorMsg = "设置失败: " + parseAppError(e);
    } finally {
      pwdSubmitting = false;
    }
  }

  function handlePinClear() {
    pwdErrorMsg = "";
    pwdSuccess = false;
  }

  async function clearPassword() {
    try {
      await resetPassword();
      knownPassword = "";
      onStatusChange({ enabled: false, has_password: false });
      pwdPanelExpanded = false;
      pwdErrorMsg = "";
      pwdSuccess = false;
    } catch (e) {
      onError("清除密码失败: " + parseAppError(e));
    }
  }

  function togglePwdPanel() {
    pwdPanelExpanded = !pwdPanelExpanded;
    pwdErrorMsg = "";
    pwdSuccess = false;
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
  {#if hasPassword && knownPassword}
    <div class="pwd-show-area">
      <div class="pwd-display-row">
        <span class="pwd-hint-text">{passwordEnabled ? "访问密码" : "已保存密码"}:</span>
        <div class="pwd-digits">
          {#each knownPassword.split('') as digit}
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
  {:else if hasPassword && !knownPassword}
    <div class="pwd-setup-area">
      <div class="setup-hint">密码已设置但当前会话未记录，可重新设置新密码</div>
      <button class="setup-random-btn" onclick={randomGenerateAndApply}>随机生成新密码</button>
      <button class="setup-manual-btn" onclick={togglePwdPanel}>手动输入新密码</button>
    </div>
  {:else if !hasPassword}
    <div class="pwd-setup-area">
      <div class="setup-hint">设置4位数字密码，局域网访问需输入此密码</div>
      <button class="setup-random-btn" onclick={randomGenerateAndApply}>随机生成并启动保护</button>
      <button class="setup-manual-btn" onclick={togglePwdPanel}>手动输入密码</button>
    </div>
  {/if}

  {#if pwdPanelExpanded}
    <PinInput
      submitting={pwdSubmitting}
      errorMsg={pwdErrorMsg}
      success={pwdSuccess}
      onSubmit={handlePinSubmit}
      onClear={handlePinClear}
    />
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
</style>
