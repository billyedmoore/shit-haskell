-- An explicit Type annotation agrees with the inferred lifted kind of the instance.
module Main where
import CacheBootMonoConsumer
import CacheBootMono
main :: IO ()
main = print (throughUniversal 'x', universal True, throughDescribe 'x', describe True)
