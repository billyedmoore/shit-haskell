{-# LANGUAGE TypeFamilies, PolyKinds #-}
import Data.Proxy (Proxy)

type family T (a :: k) where
  T (a :: k) = Proxy (a :: k)

main :: IO ()
main = pure ()
