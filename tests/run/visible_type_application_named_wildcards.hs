{-# LANGUAGE NamedWildCards, NoPartialTypeSignatures, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy

value = id @([_item], Maybe _item) ([True], Just False)

main :: IO ()
main = print value
