{-# LANGUAGE NoDataKinds, TypeFamilies #-}

import Data.Kind (Type)
data Mark = MkMark
type family F a :: Mark

main :: IO ()
main = pure ()
