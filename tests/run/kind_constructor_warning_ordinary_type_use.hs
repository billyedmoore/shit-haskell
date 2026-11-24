{-# LANGUAGE NoDataKinds #-}

import Data.Kind (Type)
data Mark = MkMark
value :: Mark
value = MkMark

main :: IO ()
main = pure ()
