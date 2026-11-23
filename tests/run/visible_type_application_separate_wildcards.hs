{-# LANGUAGE NamedWildCards, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = const @_item @_item True 'x'

main :: IO ()
main = print value
