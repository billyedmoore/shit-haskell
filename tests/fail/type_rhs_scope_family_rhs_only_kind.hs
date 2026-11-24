{-# LANGUAGE TypeFamilies, PolyKinds #-}
import Data.Proxy (Proxy)

type family T a where
  T a = Proxy (a :: k)

main :: IO ()
main = pure ()
