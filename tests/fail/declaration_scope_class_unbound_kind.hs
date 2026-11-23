{-# LANGUAGE ExplicitForAll, PolyKinds #-}
import Data.Proxy
class C a where
  m :: forall b. a -> Proxy (b :: missing)
main :: IO ()
main = pure ()
