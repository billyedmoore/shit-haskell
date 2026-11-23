{-# LANGUAGE PolyKinds, ScopedTypeVariables, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value :: forall k (a :: k). Proxy a
value = Proxy @(a :: k)

main :: IO ()
main = print (value @Type @Int)
