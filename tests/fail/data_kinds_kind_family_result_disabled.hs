{-# LANGUAGE NoDataKinds, TypeFamilies #-}

import Data.Kind (Type)

type family F a :: (Type, Type)

main :: IO ()
main = pure ()
