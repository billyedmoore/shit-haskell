{-# LANGUAGE ExplicitForAll, ForeignFunctionInterface #-}
foreign import ccall "foo" f :: forall a. missing -> IO ()
main :: IO ()
main = pure ()
