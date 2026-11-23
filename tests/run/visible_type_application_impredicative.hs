{-# LANGUAGE Haskell2010, ImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = Proxy @(forall a. a -> a)

main :: IO ()
main = print value
