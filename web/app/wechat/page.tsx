import type { Metadata } from "next";
import Image from "next/image";
import { SiteFooter, SiteHeader } from "../components/site-chrome";
import { CHINA_COMMUNITY, DISCORD, LARK_JOIN, LARK_QR, WECHAT_QR } from "../links";

export const metadata: Metadata = {
  title: "中国用户交流群 · WeChat and Lark community for China",
  description: "加入 Codync 中国用户交流群，交流使用经验、提问与反馈。使用普通微信扫码即可，无需安装企业微信；加入不了的话，可以加入飞书（Lark）群。",
  alternates: { canonical: CHINA_COMMUNITY },
};

const groups = [
  {
    name: "微信",
    join: "使用普通微信扫描二维码加入，无需安装企业微信。",
    phone: "在手机上浏览？保存二维码后，在微信「扫一扫」中从相册选择图片。",
    qr: WECHAT_QR,
    file: "codync-wechat-group.png",
  },
  {
    name: "飞书（Lark）",
    join: "微信群加入不了的话，用 Lark App 扫描二维码加入。还没有账号的话，需要先下载并注册 Lark。",
    phone: "在手机上浏览？直接点下方链接，在 Lark 中打开。",
    qr: LARK_QR,
    file: "codync-lark-group.png",
    open: { href: LARK_JOIN, label: "在 Lark 中打开" },
  },
];

const linkClass = "text-neutral-100 underline underline-offset-4 hover:text-white";

export default function CommunityPage() {
  return (
    <>
      <SiteHeader />
      <main className="flex-1 px-4 py-16 sm:px-6 md:py-24" lang="zh-CN">
        <div className="mx-auto max-w-2xl">
          <p className="text-sm font-medium text-neutral-400">Codync · WeChat and Lark community for China</p>
          <h1 className="mt-4 text-3xl font-semibold tracking-tight text-neutral-50 sm:text-4xl">
            中国用户交流群
          </h1>
          <p className="mt-5 text-lg leading-relaxed text-neutral-400">
            和中国的 Codync 用户交流使用经验、提问与反馈。请先加入微信群；如果加入不了，再加入飞书（Lark）群。
          </p>
          <div className="mt-10 grid gap-12 sm:grid-cols-2 sm:gap-8">
            {groups.map((group) => (
              <section key={group.name}>
                <h2 className="text-xl font-semibold text-neutral-50">{group.name}</h2>
                <p className="mt-3 leading-relaxed text-neutral-400">{group.join}</p>
                <div className="mt-5 w-fit max-w-full rounded-2xl bg-white p-4">
                  <Image
                    src={group.qr}
                    alt={`Codync ${group.name}中国用户交流群入群二维码`}
                    width={240}
                    height={240}
                    className="h-auto w-60 max-w-full"
                  />
                </div>
                <p className="mt-5 leading-relaxed text-neutral-400">{group.phone}</p>
                <div className="mt-4 flex flex-wrap gap-x-6 gap-y-3 font-medium">
                  {group.open && (
                    <a href={group.open.href} className={linkClass}>
                      {group.open.label}
                    </a>
                  )}
                  <a href={group.qr} download={group.file} className={linkClass}>
                    下载入群二维码
                  </a>
                </div>
              </section>
            ))}
          </div>
          <a href={DISCORD} className="mt-12 inline-block font-medium text-neutral-300 underline underline-offset-4 hover:text-white" lang="en">
            Prefer Discord? Join us there
          </a>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
