import type { Metadata } from "next";
import Image from "next/image";
import { SiteFooter, SiteHeader } from "../components/site-chrome";
import { DISCORD, WECHAT, WECHAT_QR } from "../links";

export const metadata: Metadata = {
  title: "微信中国用户交流群 · WeChat community for China",
  description: "加入 Codync 微信中国用户交流群，交流使用经验、提问与反馈。使用普通微信扫码即可，无需安装企业微信。",
  alternates: { canonical: WECHAT },
};

export default function WechatPage() {
  return (
    <>
      <SiteHeader />
      <main className="flex-1 px-4 py-16 sm:px-6 md:py-24" lang="zh-CN">
        <div className="mx-auto max-w-2xl">
          <p className="text-sm font-medium text-neutral-400">Codync · WeChat community for China</p>
          <h1 className="mt-4 text-3xl font-semibold tracking-tight text-neutral-50 sm:text-4xl">
            微信中国用户交流群
          </h1>
          <p className="mt-5 text-lg leading-relaxed text-neutral-400">
            和中国的 Codync 用户交流使用经验、提问与反馈。使用普通微信扫描下方二维码即可加入「Codync 交流群」，无需安装企业微信。
          </p>
          <div className="mt-8 w-fit max-w-full rounded-2xl bg-white p-4">
            <Image
              src={WECHAT_QR}
              alt="Codync 微信中国用户交流群入群二维码"
              width={396}
              height={396}
              className="h-auto w-72 max-w-full"
            />
          </div>
          <p className="mt-5 leading-relaxed text-neutral-400">
            在手机上浏览？保存二维码后，在微信「扫一扫」中从相册选择图片。
          </p>
          <div className="mt-6 flex flex-wrap gap-x-6 gap-y-4 font-medium">
            <a href={WECHAT_QR} download="codync-wechat-group.png" className="text-neutral-100 underline underline-offset-4 hover:text-white">
              下载入群二维码
            </a>
            <a href={DISCORD} className="text-neutral-300 underline underline-offset-4 hover:text-white" lang="en">
              Prefer Discord? Join us there
            </a>
          </div>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
