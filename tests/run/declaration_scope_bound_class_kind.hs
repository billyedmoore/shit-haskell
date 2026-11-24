{-# LANGUAGE ExplicitForAll, PolyKinds, NoScopedTypeVariables #-}
import Data.Proxy (Proxy(..))

class C (a :: k) where
  m :: forall b. Proxy (a :: k) -> b -> b
instance C Bool where
  m _ = id

main :: IO ()
main = print (m (Proxy :: Proxy Bool) (17 :: Int))
