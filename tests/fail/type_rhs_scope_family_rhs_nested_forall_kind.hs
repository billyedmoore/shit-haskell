{-# LANGUAGE TypeFamilies, RankNTypes, PolyKinds #-}
import Data.Proxy (Proxy)

type family T a where
  T a = forall (x :: k). Proxy x -> Proxy a

main :: IO ()
main = pure ()
