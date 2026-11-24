{-# LANGUAGE PolyKinds #-}
import Data.Kind
import Data.Proxy
type T = (Proxy :: k -> Type)
main :: IO ()
main = print (Proxy :: T Bool)
