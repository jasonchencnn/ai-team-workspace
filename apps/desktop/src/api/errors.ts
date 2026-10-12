import { isErrorPayload } from "./commands.js";

/** 把任意拒绝值转成给用户看的文字：后端错误体用 message，其余给通用提示。 */
export function errorMessage(error: unknown): string {
  if (isErrorPayload(error)) {
    return error.message;
  }
  return "无法连接后端，请确认应用在桌面环境中运行。";
}
