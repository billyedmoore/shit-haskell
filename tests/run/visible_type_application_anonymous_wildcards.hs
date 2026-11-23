{-# LANGUAGE NoNamedWildCards, NoPartialTypeSignatures, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = id @(_, _) (True, 'x')

main :: IO ()
main = print value
