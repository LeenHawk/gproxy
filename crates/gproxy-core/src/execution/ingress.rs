use crate::boundary::RequestCtx;

pub(crate) fn strip(ctx: &mut RequestCtx) {
    super::forwarding::strip_ingress(ctx);
}
