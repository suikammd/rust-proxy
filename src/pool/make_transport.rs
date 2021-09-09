pub struct PooledMakeTransport<MT, T> {
    inner: MT,
    pool: Pool<ThriftTransport<T>>,
}