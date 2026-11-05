"""Placeholder pages retained from the original Hub navigation."""

cl def:pub ComingSoon(title: str, description: str, icon: str) -> JsxElement {
    return <section className="jac-coming-soon">
        <div className="jac-coming-soon-icon" aria-hidden="true">{icon}</div>
        <h1>{title}</h1>
        <p>{description}</p>
        <span>COMING SOON</span>
    </section>;
}
