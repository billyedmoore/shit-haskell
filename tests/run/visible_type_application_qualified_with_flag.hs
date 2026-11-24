{-# LANGUAGE ImpredicativeTypes, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = Proxy @(Eq Int => Int)

main :: IO ()
main = print value
