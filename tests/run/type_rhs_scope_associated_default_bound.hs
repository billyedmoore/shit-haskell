{-# LANGUAGE TypeFamilies, PolyKinds #-}
import Data.Proxy (Proxy(..))

class C a where
  type T a
  type T a = Proxy a
instance C Int

main :: IO ()
main = print (Proxy :: T Int)
