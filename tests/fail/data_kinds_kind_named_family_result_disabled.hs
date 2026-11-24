{-# LANGUAGE NoDataKinds, TypeFamilyDependencies #-}

import Data.Kind (Type)

type family F a = (r :: (Type, Type)) | r -> a

main :: IO ()
main = pure ()
