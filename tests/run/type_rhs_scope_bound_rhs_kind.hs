{-# LANGUAGE PolyKinds #-}
import Data.Proxy
type T (a :: k) = Proxy (a :: k)
main :: IO ()
main = print (Proxy :: T Bool)
