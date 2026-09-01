use crate::boundary::RequestCtx;

pub(crate) fn strip(ctx: &mut RequestCtx) {
    ctx.headers.remove("x-gproxy-session-id");
    super::forwarding::strip_ingress(ctx);
}
