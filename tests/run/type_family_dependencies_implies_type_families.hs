{-# LANGUAGE TypeFamilyDependencies #-}

type family Result a = r | r -> a
type instance Result Int = Bool

answer :: Result Int
answer = True

main :: IO ()
main = print answer
