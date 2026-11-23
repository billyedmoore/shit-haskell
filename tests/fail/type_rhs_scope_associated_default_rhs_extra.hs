{-# LANGUAGE TypeFamilies, MultiParamTypeClasses #-}
import Data.Proxy (Proxy)

class C a b where
  type T a
  type T a = Maybe b

main :: IO ()
main = pure ()
