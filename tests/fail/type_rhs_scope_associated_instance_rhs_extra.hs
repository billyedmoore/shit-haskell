{-# LANGUAGE TypeFamilies, MultiParamTypeClasses #-}
import Data.Proxy (Proxy)

class C a b where
  type T a
instance C Int (Maybe b) where
  type T Int = Maybe b

main :: IO ()
main = pure ()
